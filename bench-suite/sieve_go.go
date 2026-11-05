// Sieve, identical work: mark only up to sqrt(N), then count all primes.
// Expected: 664579.
package main

import "fmt"

const n = 100000000

func main() {
	s := make([]byte, n)
	for i := range s {
		s[i] = 1
	}

	for i := 2; i*i < n; i++ {
		if s[i] == 1 {
			for j := i * i; j < n; j += i {
				s[j] = 0
			}
		}
	}

	count := 0
	for i := 2; i < n; i++ {
		if s[i] == 1 {
			count++
		}
	}
	fmt.Println(count)
}
