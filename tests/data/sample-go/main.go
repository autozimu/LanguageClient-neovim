package main

import (
	"fmt"
	"log"
)

func main() {
	fmt.Println("Hello, 世界")
	log.Println(greet())
}

func greet() int32 {
	return 42
}

func yo() {
	someVar := 3
	b := someVar + someVar
	log.Println(b)
	log.Println(otherYo())
}

func yoWithDiagnostic() {
	x := 0
	x = x
}
