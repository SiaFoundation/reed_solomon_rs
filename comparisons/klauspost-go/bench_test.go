// klauspost/reedsolomon under Sia's defaults (10 of 30, 4 MiB shards), for
// the README's Rust vs Go comparison row.
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

func BenchmarkEncode(b *testing.B) {
	enc, err := reedsolomon.New(dataShards, parityShards)
	if err != nil {
		b.Fatal(err)
	}
	shards := makeShards(b)
	b.SetBytes(int64(totalShards) * int64(shardSize))
	b.ResetTimer()
	for b.Loop() {
		if err := enc.Encode(shards); err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkVerify(b *testing.B) {
	enc, err := reedsolomon.New(dataShards, parityShards)
	if err != nil {
		b.Fatal(err)
	}
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

func benchmarkReconstruct(b *testing.B, drop int) {
	enc, err := reedsolomon.New(dataShards, parityShards)
	if err != nil {
		b.Fatal(err)
	}
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
