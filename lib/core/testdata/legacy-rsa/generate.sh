#!/usr/bin/env bash
# Regenerates the legacy RSA fixtures: a self-signed certificate with a
# 1024-bit key (like older iLO firmware ships) and PKCS#1 signatures over
# `message.txt`. The private key is thrown away; only public material is kept.
set -euo pipefail

cd "$(dirname "$0")"
key=$(mktemp)
trap 'rm -f "$key"' EXIT

openssl genrsa -out "$key" 1024
openssl req -new -x509 -key "$key" -subj "/CN=legacy-rsa" -days 36500 -outform DER -out cert.der
for digest in sha1 sha512; do
    openssl dgst "-$digest" -sign "$key" -out "signature-$digest.bin" message.txt
done
