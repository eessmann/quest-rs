#!/usr/bin/env python3
"""Source admission and build-receipt regression tests; no cluster is needed."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location('cirrus_snapshot', HERE / 'snapshot.py')
SNAPSHOT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SNAPSHOT)


class SnapshotAdmission(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='quest-snapshot-test-')
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.root = self.base / 'repo'
        self.root.mkdir()
        for arguments in [
            ['init', '-q'], ['config', 'user.name', 'Fixture'],
            ['config', 'user.email', 'fixture@example.invalid'],
        ]:
            self.git(*arguments)
        (self.root / 'input.rs').write_text('source before build\n')
        verifier = self.root / 'docs/verification/fixtures/cirrus/snapshot.py'
        verifier.parent.mkdir(parents=True)
        verifier.write_text((HERE / 'snapshot.py').read_text())
        (verifier.parent / 'runtime-env.sh').write_text((HERE / 'runtime-env.sh').read_text())
        self.git('add', '.')
        self.git('-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture')

    def git(self, *arguments):
        return subprocess.check_output(['git', '-C', str(self.root), *arguments])

    def extract(self):
        with contextlib.redirect_stdout(io.StringIO()):
            receipt = SNAPSHOT.snapshot(self.root, self.base / 'archives')
        with tarfile.open(receipt['archive']) as archive:
            archive.extractall(self.base / 'extracted', filter='data')
        return self.base / 'extracted/source', receipt['digest']

    def test_verify_accepts_an_extracted_snapshot_and_read_only_copy(self):
        source, digest = self.extract()
        self.assertEqual(SNAPSHOT.verify(source)['digest'], digest)
        (source / 'input.rs').chmod(0o444)
        self.assertEqual(SNAPSHOT.verify(source)['digest'], digest)

    def test_verify_rejects_changed_file_content_size_mode_and_inventory(self):
        source, _ = self.extract()
        original = (source / 'input.rs').read_bytes()
        for mutation in ('content', 'size', 'mode', 'extra', 'symlink'):
            with self.subTest(mutation=mutation):
                path = source / 'input.rs'
                if mutation == 'content':
                    path.write_bytes(b'x' * len(original))
                elif mutation == 'size':
                    path.write_bytes(original + b'new')
                elif mutation == 'mode':
                    path.chmod(0o755)
                elif mutation == 'extra':
                    (source / 'extra.rs').write_text('unlisted')
                else:
                    (source / 'extra').symlink_to(self.root, target_is_directory=True)
                with self.assertRaises(ValueError):
                    SNAPSHOT.verify(source)
                path.write_bytes(original)
                path.chmod(0o644)
                for extra in ('extra.rs', 'extra'):
                    if (source / extra).exists() or (source / extra).is_symlink():
                        (source / extra).unlink()

    def test_verify_rejects_manifest_digest_tampering(self):
        source, _ = self.extract()
        path = source / 'source-snapshot.json'
        manifest = json.loads(path.read_text())
        manifest['files'][0]['sha256'] = '0' * 64
        path.write_text(json.dumps(manifest))
        with self.assertRaises(ValueError):
            SNAPSHOT.verify(source)

    def test_snapshot_rejects_new_files_created_during_collection(self):
        original = subprocess.check_output

        def change_after_listing(arguments, **kwargs):
            output = original(arguments, **kwargs)
            if 'ls-files' in arguments:
                (self.root / 'new.rs').write_text('new source')
            return output

        with patch.object(SNAPSHOT.subprocess, 'check_output', change_after_listing):
            with self.assertRaises(ValueError):
                SNAPSHOT.snapshot(self.root, self.base / 'archives')

    def build_environment(self, source):
        binary = self.base / 'bin'
        binary.mkdir()
        # Only unavailable module/compiler/build commands are substituted; the
        # production shell script and source verifier execute unchanged.
        commands = {
            'module': '#!/bin/sh\nexit 0\n',
            'CC': '#!/bin/sh\necho fixture\n',
            'rustc': '#!/bin/sh\necho fixture\n',
            'cargo': ('#!/bin/sh\nprintf invoked >> "$QUEST_TEST_CARGO_LOG"\n'
                      'if [ "${QUEST_TEST_TAMPER:-0}" = 1 ]; then '
                      'printf changed > "$QUEST_SNAPSHOT/input.rs"; fi\n'
                      'exit "${QUEST_TEST_CARGO_STATUS:-0}"\n'),
        }
        for name, text in commands.items():
            path = binary / name
            path.write_text(text)
            path.chmod(0o755)
        environment = {key: value for key, value in os.environ.items()
                       if not key.startswith('BASH_FUNC_') and key != 'BASH_ENV'}
        environment.update(
            PATH=str(binary) + os.pathsep + os.environ['PATH'],
            QUEST_CAMPAIGN_ROOT=str(self.base / 'campaign'),
            QUEST_SNAPSHOT=str(source), QUEST_COMPILER='cray',
            SLURM_CPUS_PER_TASK='1', SLURM_JOB_ID='42',
            QUEST_TEST_CARGO_LOG=str(self.base / 'cargo.log'),
        )
        environment.pop('QUEST_SNAPSHOT_VERIFIER', None)
        return environment

    def run_build(self, environment, script=None):
        return subprocess.run(
            ['bash', '--noprofile', '--norc', str(script or HERE / 'build-rust.sh')],
            env=environment, capture_output=True, text=True,
        )

    def module_profile_fixture(self, environment):
        environment['QUEST_TEST_MODULE_STATE'] = str(self.base / 'modules.json')
        module = self.base / 'bin/module'
        module.write_text('''#!/usr/bin/env python3
import json,os,pathlib,sys
path=pathlib.Path(os.environ['QUEST_TEST_MODULE_STATE'])
state=json.loads(path.read_text()) if path.exists() else {}
args=sys.argv[1:]
if args == ['restore']:
    state=dict(compiler='cray',libsci=True)
elif args == ['switch','PrgEnv-cray','PrgEnv-gnu']:
    state['compiler']='gnu'
elif args == ['unload','cray-libsci']:
    state['libsci']=False
elif args == ['-t','list']:
    print(json.dumps(state))
path.write_text(json.dumps(state))
''')

    def test_runtime_profile_excludes_libsci_only_for_cray(self):
        source, _ = self.extract()
        environment = self.build_environment(source)
        self.module_profile_fixture(environment)
        for compiler in ('cray', 'gnu'):
            with self.subTest(compiler=compiler):
                environment['QUEST_COMPILER'] = compiler
                result = self.run_build(environment)
                self.assertEqual(result.returncode, 0, result.stderr)
                profile, = (Path(environment['QUEST_CAMPAIGN_ROOT']) /
                            'targets' / compiler).rglob('.native-build-profile.json')
                self.assertEqual(json.loads(json.loads(profile.read_text())['modules']),
                                 dict(compiler=compiler, libsci=compiler != 'cray'))

    def test_native_profile_excludes_libsci_without_disabling_openmp(self):
        source, _ = self.extract()
        environment = self.build_environment(source)
        self.module_profile_fixture(environment)
        environment['QUEST_NATIVE_SOURCE'] = str(self.root)
        environment['QUEST_NATIVE_REVISION'] = self.git('rev-parse', 'HEAD').decode().strip()
        environment['QUEST_TEST_NATIVE_CONFIGURES'] = str(self.base / 'native-configures.jsonl')
        cmake = self.base / 'bin/cmake'
        cmake.write_text('''#!/usr/bin/env python3
import json,os,pathlib,sys
if '-S' in sys.argv:
    state=json.loads(pathlib.Path(os.environ['QUEST_TEST_MODULE_STATE']).read_text())
    state['openmp']='-DQUEST_ENABLE_OMP=ON' in sys.argv
    with open(os.environ['QUEST_TEST_NATIVE_CONFIGURES'],'a') as output:
        output.write(json.dumps(state)+'\\n')
''')
        cmake.chmod(0o755)
        result = self.run_build(environment, HERE / 'build-native.sh')
        self.assertEqual(result.returncode, 0, result.stderr)
        configurations = [json.loads(line) for line in
                          Path(environment['QUEST_TEST_NATIVE_CONFIGURES']).read_text().splitlines()]
        self.assertEqual(configurations, [dict(compiler='gnu', libsci=True, openmp=True),
                                          dict(compiler='cray', libsci=False, openmp=True)])

    def test_build_from_slurm_spool_finds_shared_snapshot_verifier(self):
        source, digest = self.extract()
        environment = self.build_environment(source)
        spool = self.base / 'spool/slurm_script'
        spool.parent.mkdir()
        spool.write_text((HERE / 'build-rust.sh').read_text())
        result = self.run_build(environment, spool)
        self.assertEqual(result.returncode, 0, result.stderr)
        marker, = (self.base / 'campaign').rglob('build-complete')
        self.assertEqual(marker.read_text().strip(), digest)

    def test_build_preserves_rust_flags_and_selects_cray_link_policy(self):
        source, _ = self.extract()
        environment = self.build_environment(source)
        capture = self.base / 'flags.json'
        cargo = self.base / 'bin/cargo'
        cargo.write_text('#!/usr/bin/env python3\nimport json,os\n'
                         f'open({str(capture)!r},"w").write(json.dumps(dict('
                         'encoded=os.environ.get("CARGO_ENCODED_RUSTFLAGS"),'
                         'plain=os.environ.get("RUSTFLAGS"))))\n')
        encoded = '-C\x1flink-arg=-Wl,-rpath,/sdk with spaces/lib'
        cray_policy = '-C\x1flinker-features=-lld\x1f-C\x1flink-arg=-mno-daz-ftz'
        for ordinal, (compiler, initial, expected) in enumerate((
                ('cray', encoded, encoded + '\x1f' + cray_policy),
                ('cray', '', cray_policy),
                ('cray', None, '-C\x1ftarget-cpu=native\x1f' + cray_policy),
                ('gnu', encoded, encoded),
                ('gnu', None, None))):
            with self.subTest(compiler=compiler, initial=initial):
                environment['QUEST_CAMPAIGN_ROOT'] = str(self.base / f'campaign-{ordinal}')
                environment['QUEST_COMPILER'] = compiler
                environment['RUSTFLAGS'] = '-C target-cpu=native'
                environment.pop('CARGO_ENCODED_RUSTFLAGS', None)
                if initial is not None:
                    environment['CARGO_ENCODED_RUSTFLAGS'] = initial
                result = self.run_build(environment)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(capture.read_text()), {
                    'encoded': expected,
                    'plain': None if compiler == 'cray' else '-C target-cpu=native',
                })
                profile, = Path(environment['QUEST_CAMPAIGN_ROOT']).rglob(
                    '.native-build-profile.json')
                self.assertEqual(json.loads(profile.read_text())['rustflags'],
                                 expected.split('\x1f') if expected is not None
                                 else ['-C', 'target-cpu=native'])

    def test_different_snapshots_cannot_reuse_the_same_cargo_target(self):
        source, first_digest = self.extract()
        environment = self.build_environment(source)
        first = self.run_build(environment)
        self.assertEqual(first.returncode, 0, first.stderr)
        (self.root / 'input.rs').write_text('source changed in a later snapshot\n')
        source, second_digest = self.extract()
        second = self.run_build(environment)
        self.assertEqual(second.returncode, 0, second.stderr)
        recorded = [path.read_text().strip() for path in
                    (self.base / 'campaign/receipts/cray').glob('*/target-directory')]
        self.assertEqual(set(recorded), {
            str(self.base / 'campaign/targets/cray' / first_digest),
            str(self.base / 'campaign/targets/cray' / second_digest),
        })

    def test_cray_rustdoc_policy_preserves_its_own_encoded_flag_precedence(self):
        source, _ = self.extract()
        environment = self.build_environment(source)
        capture = self.base / 'rustdoc-flags.json'
        (self.base / 'bin/cargo').write_text(
            '#!/usr/bin/env python3\nimport json,os\n'
            f'open({str(capture)!r},"w").write(json.dumps(dict('
            'encoded=os.environ.get("CARGO_ENCODED_RUSTDOCFLAGS"),'
            'plain=os.environ.get("RUSTDOCFLAGS"))))\n')
        encoded = '--cfg\x1fdocumented_platform\x1f-C\x1flink-arg=/sdk with spaces/lib'
        policy = '-C\x1flinker-features=-lld\x1f-C\x1flink-arg=-mno-daz-ftz'
        for ordinal, (compiler, initial, expected) in enumerate((
                ('cray', encoded, encoded + '\x1f' + policy),
                ('cray', '', policy),
                ('cray', None, '--cfg\x1fplain_documentation\x1f' + policy),
                ('gnu', encoded, encoded),
                ('gnu', '', ''),
                ('gnu', None, None))):
            with self.subTest(compiler=compiler, initial=initial):
                environment['QUEST_CAMPAIGN_ROOT'] = str(self.base / f'doc-campaign-{ordinal}')
                environment['QUEST_COMPILER'] = compiler
                environment['CARGO_ENCODED_RUSTFLAGS'] = '--cfg\x1fcompiler_only'
                environment['RUSTDOCFLAGS'] = '--cfg plain_documentation'
                environment.pop('CARGO_ENCODED_RUSTDOCFLAGS', None)
                if initial is not None:
                    environment['CARGO_ENCODED_RUSTDOCFLAGS'] = initial
                result = self.run_build(environment)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(capture.read_text()), {
                    'encoded': expected,
                    'plain': None if compiler == 'cray' else '--cfg plain_documentation',
                })
                profile, = Path(environment['QUEST_CAMPAIGN_ROOT']).rglob(
                    '.native-build-profile.json')
                self.assertEqual(json.loads(profile.read_text())['rustdocflags'],
                                 expected.split('\x1f') if expected else []
                                 if expected is not None else ['--cfg', 'plain_documentation'])

    def test_changed_rustdoc_flags_cannot_reuse_an_admitted_target_profile(self):
        source, _ = self.extract()
        environment = self.build_environment(source)
        first = self.run_build(environment)
        self.assertEqual(first.returncode, 0, first.stderr)
        log = self.base / 'cargo.log'
        before = log.read_text()
        environment['CARGO_ENCODED_RUSTDOCFLAGS'] = '--cfg\x1fchanged_documentation_profile'
        self.assertNotEqual(self.run_build(environment).returncode, 0)
        self.assertEqual(log.read_text(), before)

    def test_changed_compiler_flags_cannot_reuse_an_admitted_target_profile(self):
        source, _ = self.extract()
        environment = self.build_environment(source)
        first = self.run_build(environment)
        self.assertEqual(first.returncode, 0, first.stderr)
        log = self.base / 'cargo.log'
        before = log.read_text()
        environment['CARGO_ENCODED_RUSTFLAGS'] = '--cfg\x1fchanged_build_profile'
        self.assertNotEqual(self.run_build(environment).returncode, 0)
        self.assertEqual(log.read_text(), before)

    def test_failed_retry_has_its_own_receipt_without_stale_success(self):
        source, digest = self.extract()
        environment = self.build_environment(source)
        first = self.run_build(environment)
        self.assertEqual(first.returncode, 0, first.stderr)
        markers = list((self.base / 'campaign/receipts/cray').rglob('build-complete'))
        self.assertEqual(len(markers), 1)
        self.assertEqual(markers[0].read_text().strip(), digest)
        environment['QUEST_TEST_CARGO_STATUS'] = '37'
        second = self.run_build(environment)
        self.assertEqual(second.returncode, 37, second.stderr)
        receipts = list((self.base / 'campaign/receipts/cray').glob('*/source-snapshot.json'))
        self.assertEqual(len(receipts), 2)
        self.assertEqual(list((self.base / 'campaign').rglob('build-complete')), markers)

    def test_build_rejects_tampering_before_and_after_cargo(self):
        source, _ = self.extract()
        environment = self.build_environment(source)
        original = (source / 'input.rs').read_text()
        (source / 'input.rs').write_text('changed before build')
        self.assertNotEqual(self.run_build(environment).returncode, 0)
        self.assertFalse((self.base / 'cargo.log').exists())
        (source / 'input.rs').write_text(original)
        environment['QUEST_TEST_TAMPER'] = '1'
        self.assertNotEqual(self.run_build(environment).returncode, 0)
        self.assertTrue((self.base / 'cargo.log').exists())
        self.assertEqual(list((self.base / 'campaign').rglob('build-complete')), [])


if __name__ == '__main__':
    unittest.main()
