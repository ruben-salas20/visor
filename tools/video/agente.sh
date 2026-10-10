#!/usr/bin/env bash
# The agent's side of the demo, run by demo.sh inside a small foot window. It types each
# command as it runs it, for real, against the isolated instance. Output is cut to the
# fields worth reading in a GIF with jq.
#   agente.sh <visor> <go file> <done file>
# Reads PAGINA, NOTA and RESPUESTA from the environment.
set -euo pipefail
VISOR=$1 GO=$2 DONE=$3

prompt() { printf '\e[38;2;230;142;13m❯\e[0m '; }
type_out() { # Prints a command one character at a time.
  local s=$1 i
  for ((i = 0; i < ${#s}; i++)); do printf '%s' "${s:i:1}"; sleep 0.022; done
  sleep 0.25
  printf '\n'
}
dim() { printf '\e[38;2;140;140;140m%s\e[0m\n' "$1"; }

printf '\e[?25l' # no cursor: it would blink between frames
clear
prompt
while [ ! -e "$GO" ]; do sleep 0.05; done
sleep 0.3

type_out "visor open $PAGINA"
dim "$("$VISOR" open "$PAGINA" | jq -c '{ok, file, loaded, problems}')"
after=$("$VISOR" status | jq .last_event_id)
prompt
sleep 0.6

type_out "visor events --wait 300"
ev=$("$VISOR" events --after "$after" --wait 300 --limit 1)
dim "$(jq -c '.events[0] | {file, annotation}' <<<"$ev")"
id=$(jq -r '.events[0].annotation.id' <<<"$ev")
prompt
sleep 1.2

type_out "visor notes reply $id \"$RESPUESTA\""
dim "$("$VISOR" notes reply "$id" "$RESPUESTA" | jq -c .)"
prompt
sleep 3.2

type_out "visor open $NOTA --beside"
dim "$("$VISOR" open "$NOTA" --beside | jq -c '{ok, file, loaded, problems}')"
prompt
touch "$DONE"
sleep 30
