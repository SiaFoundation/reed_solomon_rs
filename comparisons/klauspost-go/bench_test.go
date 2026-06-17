// klauspost/reedsolomon under Sia's defaults (10 of 30, 4 MiB shards), for
// the README's Rust vs Go comparison row.
//
// Each operation is benched in three modes: the default Vandermonde matrix
// codec, and the Leopard FFT codec forced on in GF(2^8) and GF(2^16) (Leopard
// normally only kicks in above 256 shards; the options force it below that).
// Run: `go test -bench . -benchtime=5s ./...`
package klauspostbench

import (
	"crypto/rand"
	"testing"

	"github.com/klauspost/reedsolomon"
	"go.sia.tech/core/rhp/v4"
)

const (
	dataShards   = 10
	parityShards = 20
	totalShards  = dataShards + parityShards
	shardSize    = rhp.SectorSize
)

func makeShards(tb testing.TB) [][]byte {
	shards := make([][]byte, totalShards)
	for i := range shards {
		shards[i] = make([]byte, shardSize)
		if i < dataShards {
			if _, err := rand.Read(shards[i]); err != nil {
				tb.Fatal(err)
			}
		}
	}
	return shards
}

func newEncoder(tb testing.TB, opts ...reedsolomon.Option) reedsolomon.Encoder {
	enc, err := reedsolomon.New(dataShards, parityShards, opts...)
	if err != nil {
		tb.Fatal(err)
	}
	return enc
}

func benchmarkEncode(b *testing.B, opts ...reedsolomon.Option) {
	enc := newEncoder(b, opts...)
	shards := makeShards(b)
	b.SetBytes(int64(totalShards) * int64(shardSize))
	b.ResetTimer()
	for b.Loop() {
		if err := enc.Encode(shards); err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkEncode(b *testing.B)            { benchmarkEncode(b) }
func BenchmarkEncodeLeopardGF8(b *testing.B)  { benchmarkEncode(b, reedsolomon.WithLeopardGF(true)) }
func BenchmarkEncodeLeopardGF16(b *testing.B) { benchmarkEncode(b, reedsolomon.WithLeopardGF16(true)) }

func benchmarkVerify(b *testing.B, opts ...reedsolomon.Option) {
	enc := newEncoder(b, opts...)
	shards := makeShards(b)
	if err := enc.Encode(shards); err != nil {
		b.Fatal(err)
	}
	b.SetBytes(int64(totalShards) * int64(shardSize))
	b.ResetTimer()
	for b.Loop() {
		ok, err := enc.Verify(shards)
		if err != nil || !ok {
			b.Fatalf("verify failed: ok=%v err=%v", ok, err)
		}
	}
}

func BenchmarkVerify(b *testing.B)            { benchmarkVerify(b) }
func BenchmarkVerifyLeopardGF8(b *testing.B)  { benchmarkVerify(b, reedsolomon.WithLeopardGF(true)) }
func BenchmarkVerifyLeopardGF16(b *testing.B) { benchmarkVerify(b, reedsolomon.WithLeopardGF16(true)) }

func benchmarkReconstruct(b *testing.B, drop int, opts ...reedsolomon.Option) {
	enc := newEncoder(b, opts...)
	full := makeShards(b)
	if err := enc.Encode(full); err != nil {
		b.Fatal(err)
	}
	// Throughput denominator matches the Rust bench: bytes of recovered slab.
	b.SetBytes(int64(dataShards) * int64(shardSize))

	shards := make([][]byte, totalShards)
	for i := range shards {
		shards[i] = make([]byte, shardSize)
	}
	b.ResetTimer()
	for b.Loop() {
		b.StopTimer()
		for j := range shards {
			if j < drop {
				shards[j] = nil
			} else {
				if shards[j] == nil {
					shards[j] = make([]byte, shardSize)
				}
				copy(shards[j], full[j])
			}
		}
		b.StartTimer()
		if err := enc.ReconstructData(shards); err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkReconstruct1Data(b *testing.B)  { benchmarkReconstruct(b, 1) }
func BenchmarkReconstruct10Data(b *testing.B) { benchmarkReconstruct(b, 10) }

func BenchmarkReconstruct1DataLeopardGF8(b *testing.B) {
	benchmarkReconstruct(b, 1, reedsolomon.WithLeopardGF(true))
}
func BenchmarkReconstruct1DataLeopardGF16(b *testing.B) {
	benchmarkReconstruct(b, 1, reedsolomon.WithLeopardGF16(true))
}
func BenchmarkReconstruct10DataLeopardGF8(b *testing.B) {
	benchmarkReconstruct(b, 10, reedsolomon.WithLeopardGF(true))
}
func BenchmarkReconstruct10DataLeopardGF16(b *testing.B) {
	benchmarkReconstruct(b, 10, reedsolomon.WithLeopardGF16(true))
}
