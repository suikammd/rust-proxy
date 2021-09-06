#!/bin/sh

parameter=""

if [ ! -z "$MODE" ]
then
      parameter="$parameter --mode $MODE"
fi

if [ ! -z "$LISTEN" ]
then
      parameter="$parameter --listen_addr $LISTEN"
fi

if [ ! -z "$FULLCHAIN" ]
then
      parameter="$parameter --fullchain $FULLCHAIN"
fi

if [ ! -z "$PROXY" ]
then
      parameter="$parameter --proxy_addr $PROXY"
fi

if [ ! -z "$PRIVATE_KEY" ]
then
      parameter="$parameter --private_key $PRIVATE_KEY"
fi

if [ ! -z "$AUTHORIZATION" ]
then
      parameter="$parameter --private_key $AUTHORIZATION"
fi

ss $parameter