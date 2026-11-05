package main

import "fmt"

func main() {
	const size = 2097152
	const n = 1000000
	keys := make([]int64, size)
	vals := make([]int64, size)

	for i := int64(1); i <= n; i++ {
		slot := (i * 2654435761) % size
		for keys[slot] != 0 {
			slot = (slot + 1) % size
		}
		keys[slot] = i
		vals[slot] = i * 3
	}

	var total int64
	for i := int64(1); i <= n; i++ {
		slot := (i * 2654435761) % size
		for keys[slot] != i {
			slot = (slot + 1) % size
		}
		total += vals[slot]
	}
	fmt.Println(total)
}
