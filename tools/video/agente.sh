#!/usr/bin/env bash
# The agent's side of the demo, run by demo.sh inside a small foot window. It types each
# command as it runs it, for real, against the isolated instance. Output is cut to the
# fields worth reading in a GIF with jq.
#   agente.sh <visor> <go file> <done file>
# Reads PAGINA, NOTA and RESPUESTA from the environment.
set -euo pipefail
VISOR=$1 GO=$2 DONE=$3

prompt() { printf '\e[38;2;230;142;13m❯\e[0m '; }
COLS=$(tput cols)
type_out() { # Prints a command one character at a time, wrapping between words.
  local s=$1 i col=2 word
  for word in $s; do
    if ((col > 2 && col + 1 + ${#word} > COLS)); then printf '\n  '; col=2
    elif ((col > 2)); then printf ' '; ((col += 1)); fi
    for ((i = 0; i < ${#word}; i++)); do printf '%s' "${word:i:1}"; sleep 0.022; done
    ((col += ${#word}))
  done
  sleep 0.25
  printf '\n'
}
dim() { printf '\e[38;2;140;140;140m%s\e[0m\n' "$(fold -s -w "$COLS" <<<"$1")"; }

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
