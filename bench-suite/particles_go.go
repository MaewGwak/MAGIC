package main

import "fmt"

type P struct {
	x, y, vx, vy float64
}

func main() {
	const N = 1000000
	const steps = 100
	p := make([]P, N)
	for i := 0; i < N; i++ {
		p[i] = P{float64(i), float64(i) * 0.5, 1.0, 0.25}
	}
	for s := 0; s < steps; s++ {
		for i := range p {
			p[i].x += p[i].vx
			p[i].y += p[i].vy
			p[i].vx *= 0.999
			p[i].vy *= 0.999
		}
	}
	var total float64
	for i := range p {
		total += p[i].x
	}
	fmt.Printf("%.3f\n", total)
}
