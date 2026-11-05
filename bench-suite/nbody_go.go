package main

import (
	"fmt"
	"math"
)

const NB = 4
const STEPS = 10000000

func main() {
	x := [NB]float64{0, 2, 4, 6}
	y := [NB]float64{0, 1, -1, 0.5}
	z := [NB]float64{0, 0.1, -0.1, 0.2}
	vx := [NB]float64{0, 0.01, -0.01, 0.02}
	vy := [NB]float64{0, 0.005, -0.005, 0.001}
	vz := [NB]float64{0, 0.001, -0.001, 0.0005}
	m := [NB]float64{100.0, 0.5, 0.3, 0.2}
	const dt = 0.001

	for s := 0; s < STEPS; s++ {
		for i := 0; i < NB; i++ {
			for j := i + 1; j < NB; j++ {
				dx := x[i] - x[j]
				dy := y[i] - y[j]
				dz := z[i] - z[j]
				d2 := dx*dx + dy*dy + dz*dz
				d := math.Sqrt(d2)
				mag := dt / (d2 * d)
				vx[i] -= dx * m[j] * mag
				vy[i] -= dy * m[j] * mag
				vz[i] -= dz * m[j] * mag
				vx[j] += dx * m[i] * mag
				vy[j] += dy * m[i] * mag
				vz[j] += dz * m[i] * mag
			}
		}
		for i := 0; i < NB; i++ {
			x[i] += dt * vx[i]
			y[i] += dt * vy[i]
			z[i] += dt * vz[i]
		}
	}

	e := 0.0
	for i := 0; i < NB; i++ {
		e += 0.5 * m[i] * (vx[i]*vx[i] + vy[i]*vy[i] + vz[i]*vz[i])
	}
	for i := 0; i < NB; i++ {
		for j := i + 1; j < NB; j++ {
			dx := x[i] - x[j]
			dy := y[i] - y[j]
			dz := z[i] - z[j]
			e -= m[i] * m[j] / math.Sqrt(dx*dx+dy*dy+dz*dz)
		}
	}
	fmt.Printf("%.9f\n", e)
}
