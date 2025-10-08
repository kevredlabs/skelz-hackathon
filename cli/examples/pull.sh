#!/bin/bash

echo "=== Pulling signature artifact ==="
oras pull ghcr.io/kevredlabs/cypherpunk-demo:skelz-proof-1

echo -e "\n=== Displaying JSON content ==="
cat ./downloaded-signature/signature.json | jq .

echo -e "\n=== Displaying artifact manifest ==="
oras manifest get ghcr.io/kevredlabs/cypherpunk-demo:skelz-proof-1 | jq .

echo -e "\n=== Displaying annotations ==="
oras manifest get ghcr.io/kevredlabs/cypherpunk-demo:skelz-proof-1 | jq '.annotations'
