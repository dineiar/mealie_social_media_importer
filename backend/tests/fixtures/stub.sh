#!/bin/sh
case "$1" in
speak)
    echo "out $2"
    echo "err $2" >&2
    ;;
exit)
    echo "partial"
    exit "$2"
    ;;
die)
    kill -9 $$
    ;;
cwd)
    pwd
    ;;
stdin)
    cat
    echo end
    ;;
flood)
    head -c 3145728 /dev/zero | tr '\0' e >&2
    head -c 3145728 /dev/zero | tr '\0' o
    ;;
hang | leave)
    (while :; do echo tick >>"$3"; sleep 0.05; done) &
    until [ -s "$3" ]; do sleep 0.01; done
    echo $$ >"$2"
    echo started
    if [ "$1" = hang ]; then
        wait
    fi
    ;;
esac
