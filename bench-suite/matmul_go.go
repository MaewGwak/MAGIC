package main

import "fmt"

func main() {
	const N = 384
	a := make([]int32, N*N)
	b := make([]int32, N*N)
	c := make([]int32, N*N)

	for i := 0; i < N; i++ {
		for j := 0; j < N; j++ {
			a[i*N+j] = int32((i + j) % 10)
			b[i*N+j] = int32((i * j) % 7)
		}
	}

	for i := 0; i < N; i++ {
		for j := 0; j < N; j++ {
			var s int32
			for k := 0; k < N; k++ {
				s += a[i*N+k] * b[k*N+j]
			}
			c[i*N+j] = s
		}
	}

	var total int64
	for x := 0; x < N*N; x += 2 {
		total += int64(c[x])
	}
	fmt.Println(total)
}
