# Validate raw scientific receipts against the separately captured immutable invocation.
def integer: type == "number" and isfinite and floor == .;
def finite_number: type == "number" and isfinite;
def nonnegative_number: finite_number and . >= 0;
def nonnegative_integer: integer and . >= 0;
def observed_limit:
    type == "object" and
    ((.kind == "unlimited" and (has("bytes") | not)) or
     (.kind == "finite" and (.bytes | integer) and .bytes >= 0));
def observed_limits:
    type == "object" and .source == "/proc/self/limits: Max address space" and
    (.soft | observed_limit) and (.hard | observed_limit);

$invocation[0] as $case |
($placement | split("\n") | map(select(length > 0)) | sort) as $hosts |
([.[].node.processor_name] | sort) as $active_hosts |
($invocation | length) == 1 and
$case.evidence_kind == "scaling-only" and $case.capacity_closed == false and
$case.source_manifest_sha256 == $source and $case.native_library_sha256 == $native and
$case.executable_sha256 == $executable and $case.build_profile == "release" and
($case.allocated_nodes | integer) and $case.allocated_nodes == 8 and
($case.active_nodes | integer) and ([2, 4, 8] | index($case.active_nodes)) != null and
($case.dimension | integer) and ($case.repetitions | integer) and $case.repetitions == 3 and
(($case.series == "strong" and $case.dimension == 65536) or
 ($case.series == "weak" and $case.dimension == (8192 * $case.active_nodes))) and
$case.ranks_per_node == 1 and $case.threads == 288 and
$case.managed_rank_bytes == 4294967296 and $case.managed_node_bytes == 4294967296 and
$case.stack_bytes_per_worker == 8388608 and
(length == $case.active_nodes) and
([.[].rank] | sort) == [range(0; $case.active_nodes)] and
($hosts | length) == $case.allocated_nodes and
($hosts | unique | length) == $case.allocated_nodes and
($active_hosts | unique | length) == $case.active_nodes and
all($active_hosts[]; . as $host | ($hosts | index($host)) != null) and
all(.[];
    (.schema_version | integer) and .schema_version == 6 and
    .evidence_kind == "scaling-only" and .capacity_closed == false and
    (.rank | integer) and (.ranks | integer) and .ranks == $case.active_nodes and
    (.dimension | integer) and .dimension == $case.dimension and
    (.repetitions | integer) and .repetitions == $case.repetitions and
    (.node.leader_rank | integer) and .node.leader_rank == .rank and
    (.node.local_rank | integer) and .node.local_rank == 0 and
    (.node.local_size | integer) and .node.local_size == 1 and
    (.node.processor_name | type == "string" and length > 0) and
    (.norm | finite_number) and ((.norm - 1) | fabs) <= 1e-10 and
    (.maximum_sample_error | nonnegative_number) and .maximum_sample_error <= 1e-10 and
    (.model_rank_budget_bytes | integer) and .model_rank_budget_bytes == $case.managed_rank_bytes and
    (.model_node_budget_bytes | integer) and .model_node_budget_bytes == $case.managed_node_bytes and
    has("whole_node_enforced_memory_cap_bytes") and .whole_node_enforced_memory_cap_bytes == null and
    has("whole_node_peak_bytes") and .whole_node_peak_bytes == null and
    has("persistence_load_wire_bytes") and .persistence_load_wire_bytes == null and
    (.process_address_space_limits_before | observed_limits) and
    (.process_address_space_limits_after | observed_limits) and
    .process_address_space_limits_before == .process_address_space_limits_after and
    (has("process_address_space_cap_bytes") | not) and
    (.baseline_address_space_bytes | nonnegative_integer) and
    (.modeled_rank_envelope_bytes | nonnegative_integer) and
    .modeled_rank_envelope_bytes ==
        (.baseline_address_space_bytes + .model_rank_budget_bytes + .threading.omp_stack_allowance_bytes) and
    (.modeled_node_rank_envelope_bytes | nonnegative_integer) and
    .modeled_node_rank_envelope_bytes == .modeled_rank_envelope_bytes and
    (.global_input_entries | integer) and .global_input_entries == (2 * $case.dimension) and
    (.local_input_entries | integer) and .local_input_entries == (2 * $case.dimension / $case.active_nodes) and
    (.canonical_input_bytes | integer) and .canonical_input_bytes == (64 * $case.dimension) and
    (.local_canonical_input_bytes | integer) and
    .local_canonical_input_bytes == (64 * $case.dimension / $case.active_nodes) and
    (.local_input_sha256 | type == "string" and test("^[a-f0-9]{64}$")) and
    (.input_storage_seconds | nonnegative_number) and
    (.threading.requested_threads | integer) and .threading.requested_threads == $case.threads and
    (.threading.omp_stack_bytes_per_worker | integer) and
    .threading.omp_stack_bytes_per_worker == $case.stack_bytes_per_worker and
    (.threading.omp_stack_allowance_bytes | integer) and
    .threading.omp_stack_allowance_bytes == (($case.threads - 1) * $case.stack_bytes_per_worker) and
    .threading.omp_num_threads == "288" and .threading.omp_places == "cores" and
    .threading.omp_proc_bind == "close" and .threading.omp_dynamic == "FALSE" and
    .threading.omp_stacksize == "8388608B" and
    .threading.native_environment_multithreaded == true and
    .threading.native_register_multithreaded == true and
    (.execution_roundtrip_seconds | type == "array" and length == $case.repetitions) and
    all(.execution_roundtrip_seconds[]; nonnegative_number) and
    (.producer_sent_payload_bytes | nonnegative_integer) and
    (.execution_sent_bytes | nonnegative_integer) and
    (.execution_received_bytes | nonnegative_integer) and
    (.execution_coordination_calls | nonnegative_integer) and
    (.rss_high_water_bytes | nonnegative_integer) and
    (.address_space_high_water_bytes | nonnegative_integer) and
    (.stages | type == "object" and
        all(.preprocessing, .persistence, .load, .prepare, .execution;
            type == "object" and (.seconds | nonnegative_number) and
            (.rss_endpoint_bytes | nonnegative_integer) and
            (.rss_high_water_bytes | nonnegative_integer) and
            (.address_space_endpoint_bytes | nonnegative_integer) and
            (.address_space_high_water_bytes | nonnegative_integer)))
)
