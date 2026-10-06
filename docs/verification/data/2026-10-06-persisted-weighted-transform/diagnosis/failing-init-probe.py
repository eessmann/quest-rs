from pathlib import Path
import os,resource,subprocess,json,time,hashlib,signal
folder=Path('<private-artifacts>/quest-mpi-fsize-diagnostic')
def caps():
 os.setsid()
 resource.setrlimit(resource.RLIMIT_AS,(2147483648,2147483648))
 resource.setrlimit(resource.RLIMIT_FSIZE,(4194304,4194304))
cmd=['strace','-ff','-o',str(folder/'trace'),'-e','trace=openat,close,ftruncate,truncate,fallocate,unlink,mmap,write','/usr/lib64/mpich/bin/mpiexec','-n','8',str(folder/'init')]
t=time.monotonic()
with (folder/'stdout').open('wb') as out,(folder/'stderr').open('wb') as err:
 p=subprocess.Popen(cmd,stdout=out,stderr=err,preexec_fn=caps)
 try:code=p.wait(timeout=30)
 except subprocess.TimeoutExpired:
  os.killpg(p.pid,signal.SIGKILL);code=p.wait();raise
v={'scope':'minimal MPI_Init_thread/Finalize only; no QuEST or production consumer','returncode':code,'seconds':time.monotonic()-t,'address_space_bytes':2147483648,'file_size_bytes':4194304,'command':cmd}
for name in ['init.c','init','stdout','stderr']:
 b=(folder/name).read_bytes();v[name]={'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()}
(folder/'receipt.json').write_text(json.dumps(v,indent=2)+'\n')
print(json.dumps(v))
