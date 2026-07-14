#!/bin/bash
RUSTFLAGS="-C target-cpu=native" cargo bench --bench bench

