#!/bin/bash
sd -f m ' +$' '' <"$1" >"$1.trim"
sd -f e '\n+$' '' "$1.trim"
