#!/usr/bin/env bash
# Sign a Windows executable using jsign + Azure Key Vault.
# Works on macOS and Linux (jsign is cross-platform, Java-based).
#
# The private key never leaves Azure — jsign sends the digest to Key Vault,
# which signs it on the HSM and returns the signature.
#
# Usage:
#   ./scripts/sign-windows.sh <file-to-sign>
#   source scripts/sign-env.sh && ./scripts/sign-windows.sh dist/windows/duckier-cli.exe
#
# Required environment variables:
#   AZURE_KEY_VAULT_URI          — Key Vault URI (e.g. https://duckier.vault.azure.net/)
#   AZURE_KEY_VAULT_CERTIFICATE  — Certificate name in the vault
#   AZURE_TENANT_ID              — Azure AD tenant ID
#   AZURE_CLIENT_ID              — App registration client ID
#   AZURE_CLIENT_SECRET          — App registration client secret
#
# Optional:
#   WINDOWS_TIMESTAMP_URL — RFC 3161 timestamp server (default: DigiCert)
#
# Prerequisites:
#   brew install jsign azure-cli openjdk@17

set -euo pipefail

FILE="$1"

if [ -z "${AZURE_CLIENT_ID:-}" ] || [ -z "${AZURE_KEY_VAULT_URI:-}" ]; then
    echo "SKIP: Azure Key Vault not configured, skipping signing for: $(basename "$FILE")"
    exit 0
fi

# Check jsign is available
if ! command -v jsign &>/dev/null; then
    echo "ERROR: jsign not found. Install with: brew install jsign"
    exit 1
fi

# Check az CLI is available
if ! command -v az &>/dev/null; then
    echo "ERROR: Azure CLI not found. Install with: brew install azure-cli"
    exit 1
fi

TIMESTAMP_URL="${WINDOWS_TIMESTAMP_URL:-http://timestamp.digicert.com}"
VAULT_NAME=$(echo "$AZURE_KEY_VAULT_URI" | sed -E 's|https://([^.]+)\.vault\.azure\.net/?|\1|')
CERT_NAME="${AZURE_KEY_VAULT_CERTIFICATE:-codesigning}"

echo "Signing: $(basename "$FILE")"

# Login as service principal (silent if already logged in)
az login --service-principal \
    --username "$AZURE_CLIENT_ID" \
    --password "$AZURE_CLIENT_SECRET" \
    --tenant "$AZURE_TENANT_ID" \
    --output none 2>/dev/null

# Get access token for Key Vault
ACCESS_TOKEN=$(az account get-access-token --resource "https://vault.azure.net" --query accessToken --output tsv)

# Sign with jsign
jsign --storetype AZUREKEYVAULT \
    --keystore "$VAULT_NAME" \
    --storepass "$ACCESS_TOKEN" \
    --alias "$CERT_NAME" \
    --tsaurl "$TIMESTAMP_URL" \
    "$FILE"

echo "  Signed: $(basename "$FILE")"
