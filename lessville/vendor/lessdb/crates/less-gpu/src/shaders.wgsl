// LessDB GPU kernels: filtered sum (mask = WHERE predicate) and dot product.
// One workgroup of 256 threads per dispatch group; each thread strides the
// input, reduces within the workgroup, and thread 0 writes the workgroup
// partial. The CPU finishes the (tiny) cross-workgroup reduction.

struct Params {
    n: u32,
};

@group(0) @binding(0) var<storage, read> a: array<f32>;
@group(0) @binding(1) var<storage, read> b: array<u32>;
@group(0) @binding(2) var<storage, read_write> partials: array<f32>;
@group(0) @binding(3) var<uniform> params: Params;

var<workgroup> wsum: array<f32, 256>;

fn reduce_and_store(workgroup_id: u32, local_id: u32) {
    // Parallel reduction within the workgroup (256 -> 1).
    var stride = 128u;
    loop {
        if stride == 0u {
            break;
        }
        if local_id < stride {
            wsum[local_id] += wsum[local_id + stride];
        }
        workgroupBarrier();
        stride = stride / 2u;
    }
    if local_id == 0u {
        partials[workgroup_id] = wsum[0];
    }
}

@compute @workgroup_size(256)
fn filtered_sum(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_index) lid: u32,
    @builtin(num_workgroups) nwg: vec3<u32>,
    @builtin(workgroup_id) wgid: vec3<u32>,
) {
    let n = params.n;
    let total_threads = nwg.x * 256u;
    var sum = 0.0;
    var i = gid.x;
    loop {
        if i >= n {
            break;
        }
        sum += a[i] * f32(b[i]);
        i += total_threads;
    }
    wsum[lid] = sum;
    workgroupBarrier();
    reduce_and_store(wgid.x, lid);
}

@compute @workgroup_size(256)
fn dot(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_index) lid: u32,
    @builtin(num_workgroups) nwg: vec3<u32>,
    @builtin(workgroup_id) wgid: vec3<u32>,
) {
    let n = params.n;
    let total_threads = nwg.x * 256u;
    var sum = 0.0;
    var i = gid.x;
    loop {
        if i >= n {
            break;
        }
        sum += a[i] * bitcast<f32>(b[i]);
        i += total_threads;
    }
    wsum[lid] = sum;
    workgroupBarrier();
    reduce_and_store(wgid.x, lid);
}
