import {
    SiaHarness,
    ErasureHarness,
    FecHarness,
} from "./pkg/sia_reed_solomon_bench_wasm_comparison.js";

const SHARD_SIZE = 4 * 1024 * 1024;
const DATA = 10;
const PARITY = 20;
const STRIPE = (DATA + PARITY) * SHARD_SIZE;
const SLAB = DATA * SHARD_SIZE;

function bench(label, fn, denomBytes, minSec = 5) {
    fn();
    const start = process.hrtime.bigint();
    let iters = 0;
    let elapsed = 0n;
    const minNs = BigInt(Math.floor(minSec * 1e9));
    while (elapsed < minNs) {
        fn();
        iters++;
        elapsed = process.hrtime.bigint() - start;
    }
    const secs = Number(elapsed) / 1e9;
    const mibPerSec = (denomBytes * iters) / secs / (1024 * 1024);
    console.log(
        `  ${label.padEnd(22)} ${iters.toString().padStart(6)} iters` +
        `  ${(secs / iters * 1000).toFixed(2).padStart(8)} ms/iter` +
        `  ${mibPerSec.toFixed(1).padStart(9)} MiB/s`
    );
}

const harnesses = [
    ["sia_reed_solomon", new SiaHarness(SHARD_SIZE)],
    ["reed_solomon_erasure", new ErasureHarness(SHARD_SIZE)],
    ["fec_rs", new FecHarness(SHARD_SIZE)],
];

for (const [label, h] of harnesses) {
    if (!h.verify_iter()) throw new Error(`${label} initial verify failed`);
}

console.log(`\nshard=${SHARD_SIZE / 1024 / 1024} MiB, ${DATA}+${PARITY} (Vandermonde)\n`);

function compare(name, denom, call) {
    console.log(name);
    for (const [label, h] of harnesses) {
        bench(label, () => call(h), denom);
    }
    console.log();
}

compare("encode", STRIPE, (h) => h.encode_iter());
compare("verify", STRIPE, (h) => h.verify_iter());
compare("reconstruct -1 data", SLAB, (h) => h.reconstruct_iter(1));
compare("reconstruct -10 data", SLAB, (h) => h.reconstruct_iter(10));
