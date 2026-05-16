import { Harness } from "./pkg/sia_reed_solomon_bench_wasm.js";

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
    const totalBytes = denomBytes * iters;
    const mibPerSec = totalBytes / secs / (1024 * 1024);
    console.log(
        `${label.padEnd(28)} ${iters.toString().padStart(5)} iters` +
        `  ${(secs / iters * 1000).toFixed(2).padStart(8)} ms/iter` +
        `  ${mibPerSec.toFixed(1).padStart(8)} MiB/s`
    );
}

const h = new Harness(SHARD_SIZE);

if (!h.verify_iter()) throw new Error("initial verify failed");

console.log(`\nshard=${SHARD_SIZE / 1024 / 1024} MiB, ${DATA}+${PARITY} (Vandermonde)\n`);

bench("encode",                 () => h.encode_iter(),         STRIPE);
bench("verify",                 () => h.verify_iter(),         STRIPE);
bench("reconstruct -1 data",    () => h.reconstruct_iter(1),   SLAB);
bench("reconstruct -10 data",   () => h.reconstruct_iter(10),  SLAB);
