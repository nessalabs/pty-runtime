#!/bin/sh
# Layout torture for the terminal client.
#
# Each block targets something a grid renderer gets wrong in a different way.
# Run it inside the client and compare against the same script in a terminal
# you trust; the failure mode is almost always a column that stops lining up.
#
#   sh client/stress.sh          every block
#   sh client/stress.sh width    one block by name
set -u

esc=$(printf '\033')
ruler() {
  # A column ruler above a block makes a one-cell drift obvious.
  printf '%s[90m' "$esc"
  i=0
  while [ $i -lt 8 ]; do printf '....+....%d' $(( (i + 1) % 10 )); i=$((i + 1)); done
  printf '%s[0m\n' "$esc"
}

block_width() {
  echo "== double-width and spacer cells =="
  ruler
  echo "CJK      |日本語のテキストです|<- pipe must sit at column 21"
  echo "mixed    |a日b本c語d|<- alternating single and double"
  echo "emoji    |😀😀😀😀😀|<- five double-width emoji"
  echo "flags    |🇯🇵🇰🇷🇺🇸|<- regional indicator pairs"
  echo "zwj      |👨‍👩‍👧‍👦👩‍💻|<- zero-width-joiner families"
  echo "skin     |👋🏽👋🏿|<- emoji with modifiers"
}

block_combining() {
  echo "== combining marks and clusters =="
  ruler
  printf 'accents  |e\xcc\x81a\xcc\x80o\xcc\x82u\xcc\x88|<- each is one cell\n'
  printf 'stacked  |a\xcc\x81\xcc\xa7\xcc\x83\xcc\x84|<- four marks on one base\n'
  printf 'hangul   |\xea\xb0\x80\xeb\x82\x98\xeb\x8b\xa4|<- precomposed syllables\n'
  printf 'devanagr |\xe0\xa4\x95\xe0\xa5\x8d\xe0\xa4\xb7\xe0\xa4\xbf|<- conjunct cluster\n'
}

block_bidi() {
  echo "== right-to-left =="
  ruler
  printf 'arabic   |\xd9\x85\xd8\xb1\xd8\xad\xd8\xa8\xd8\xa7 \xd8\xa8\xd8\xa7\xd9\x84\xd8\xb9\xd8\xa7\xd9\x84\xd9\x85|<- must NOT be reordered\n'
  printf 'hebrew   |\xd7\xa9\xd7\x9c\xd7\x95\xd7\x9d \xd7\xa2\xd7\x95\xd7\x9c\xd7\x9d|<- cells stay in memory order\n'
  printf 'mixed    |abc \xd7\xa9\xd7\x9c\xd7\x95\xd7\x9d def|<- LTR around RTL\n'
}

block_boxes() {
  echo "== box drawing and blocks =="
  ruler
  echo "┌────────┬────────┐"
  echo "│ left   │ right  │"
  echo "├────────┼────────┤"
  echo "│ ▁▂▃▄▅▆▇█ │ ░▒▓█▓▒░ │"
  echo "└────────┴────────┘"
  echo "braille  |⠁⠂⠄⡀⢀⠠⠐⠈|<- eight braille cells"
  echo "powerline|▶◀◆●■|<- common prompt glyphs"
}

block_styles() {
  echo "== every attribute =="
  ruler
  for pair in "1:bold" "2:faint" "3:italic" "4:underline" "5:blink" \
              "7:inverse" "8:invisible" "9:strikethrough" "53:overline"; do
    code=${pair%%:*}; name=${pair#*:}
    printf '%s[%sm%-14s%s[0m' "$esc" "$code" "$name" "$esc"
  done
  printf '\n'
  for style in 1 2 3 4 5; do
    printf '%s[4:%sm underline-%s %s[0m' "$esc" "$style" "$style" "$esc"
  done
  printf '\n'
  printf '%s[4:3m%s[58;2;255;0;0m colored curly underline %s[0m\n' "$esc" "$esc" "$esc"
}

block_colors() {
  echo "== color =="
  ruler
  i=0
  while [ $i -lt 16 ]; do printf '%s[48;5;%dm  ' "$esc" "$i"; i=$((i + 1)); done
  printf '%s[0m <- system\n' "$esc"
  i=16
  while [ $i -lt 232 ]; do
    printf '%s[48;5;%dm ' "$esc" "$i"
    i=$((i + 1))
    [ $(( (i - 16) % 36 )) -eq 0 ] && printf '%s[0m\n' "$esc"
  done
  printf '%s[0m' "$esc"
  i=0
  while [ $i -lt 80 ]; do
    r=$(( i * 255 / 79 )); b=$(( 255 - i * 255 / 79 ))
    printf '%s[48;2;%d;0;%dm ' "$esc" "$r" "$b"
    i=$((i + 1))
  done
  printf '%s[0m <- truecolor gradient\n' "$esc"
}

block_wrap() {
  echo "== wrapping and long lines =="
  ruler
  i=0; line=""
  while [ $i -lt 240 ]; do line="$line$(( i % 10 ))"; i=$((i + 1)); done
  echo "$line"
  echo "^ 240 digits: every tenth is 0, and wrapped rows must continue the count"
}

block_churn() {
  echo "== rapid output =="
  i=0
  while [ $i -lt 400 ]; do
    printf '%s[38;5;%dmline %04d %s%s[0m\n' "$esc" $(( i % 256 )) "$i" \
      "----------------------------------------" "$esc"
    i=$((i + 1))
  done
  echo "^ 400 lines as fast as the shell can print them"
}

block_scroll() {
  echo "== scrolling region =="
  printf '%s[5;15r' "$esc"   # limit scrolling to rows 5..15
  i=0
  while [ $i -lt 30 ]; do printf '%s[15;1Hregion line %d\n' "$esc" "$i"; i=$((i + 1)); done
  printf '%s[r' "$esc"       # release the region
  printf '%s[20;1H^ only rows 5-15 should have scrolled\n' "$esc"
}

all() {
  for name in width combining bidi boxes styles colors wrap scroll churn; do
    printf '\n'
    "block_$name"
  done
}

case "${1:-all}" in
  all) all ;;
  *) "block_$1" ;;
esac
