#!/usr/bin/env bash
# Resume notarization wait + staple from saved state.
#
# Usage: ./scripts/notarize-resume.sh
#
# Reads submission IDs from dist/mac/.notarization.json (created by build-mac.sh)
# and waits for Apple approval, then staples tickets onto .pkg files.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLI_DIR="$(dirname "$SCRIPT_DIR")"

# Source signing env
SIGN_ENV="$SCRIPT_DIR/sign-env.sh"
if [ -f "$SIGN_ENV" ]; then
    source "$SIGN_ENV"
else
    echo "Error: scripts/sign-env.sh not found."
    echo "Copy scripts/sign-env.sh.example to scripts/sign-env.sh and fill in your credentials."
    exit 1
fi

NOTARIZE_STATE="$CLI_DIR/dist/mac/.notarization.json"
NOTARY_ARGS="--key $APPLE_API_KEY_PATH --key-id $APPLE_API_KEY --issuer $APPLE_API_ISSUER"

if [ ! -f "$NOTARIZE_STATE" ]; then
    echo "No saved notarization state found at:"
    echo "  $NOTARIZE_STATE"
    echo ""
    echo "This file is created by build-mac.sh --sign after submitting artifacts."
    exit 1
fi

echo "============================================"
echo " Notarization Resume"
echo "============================================"
echo ""

TIMESTAMP=$(python3 -c "import json; print(json.load(open('$NOTARIZE_STATE'))['timestamp'])")
VERSION=$(python3 -c "import json; print(json.load(open('$NOTARIZE_STATE'))['version'])")
COUNT=$(python3 -c "import json; print(len(json.load(open('$NOTARIZE_STATE'))['submissions']))")

echo "Build: v$VERSION submitted at $TIMESTAMP"
echo ""

if [ "$COUNT" -eq 0 ] 2>/dev/null; then
    echo "No submissions found in state file."
    exit 1
fi

echo "Found $COUNT submission(s):"
echo ""

FAILED=0
ACCEPTED=0

for i in $(seq 0 $((COUNT - 1))); do
    SUB_ID=$(python3 -c "import json; print(json.load(open('$NOTARIZE_STATE'))['submissions'][$i]['id'])")
    ARTIFACT=$(python3 -c "import json; print(json.load(open('$NOTARIZE_STATE'))['submissions'][$i]['artifact'])")
    NAME=$(basename "$ARTIFACT")

    echo "  [$((i+1))/$COUNT] $NAME"
    echo "       ID: $SUB_ID"

    STATUS_OUTPUT=$(xcrun notarytool info "$SUB_ID" $NOTARY_ARGS 2>&1) || true
    STATUS=$(echo "$STATUS_OUTPUT" | grep "status:" | head -1 | sed 's/.*status: //')

    echo "   Status: $STATUS"

    case "$STATUS" in
        *Accepted*)
            if [ -f "$ARTIFACT" ]; then
                echo "   Stapling..."
                xcrun stapler staple "$ARTIFACT" 2>/dev/null && echo "   Stapled OK" || echo "   Staple failed (artifact may have moved)"
            else
                echo "   WARNING: Artifact not found at $ARTIFACT"
            fi
            ACCEPTED=$((ACCEPTED + 1))
            ;;
        *"In Progress"*)
            echo "   Waiting for Apple..."
            WAIT_OUTPUT=$(xcrun notarytool wait "$SUB_ID" $NOTARY_ARGS 2>&1) || true
            if echo "$WAIT_OUTPUT" | grep -q "Accepted"; then
                echo "   Accepted! Stapling..."
                if [ -f "$ARTIFACT" ]; then
                    xcrun stapler staple "$ARTIFACT" 2>/dev/null && echo "   Stapled OK" || echo "   Staple failed"
                fi
                ACCEPTED=$((ACCEPTED + 1))
            else
                echo "   FAILED after waiting."
                echo "   Run: xcrun notarytool log $SUB_ID $NOTARY_ARGS"
                FAILED=$((FAILED + 1))
            fi
            ;;
        *Invalid*|*Rejected*)
            echo "   FAILED - check log:"
            echo "   xcrun notarytool log $SUB_ID $NOTARY_ARGS"
            FAILED=$((FAILED + 1))
            ;;
        *)
            echo "   Unknown status: $STATUS"
            FAILED=$((FAILED + 1))
            ;;
    esac
    echo ""
done

echo "============================================"
echo " Results: $ACCEPTED accepted, $FAILED failed"
if [ $FAILED -eq 0 ] && [ $ACCEPTED -gt 0 ]; then
    echo " All artifacts notarized and stapled!"
    echo " Artifacts in: dist/mac/"
fi
echo "============================================"
