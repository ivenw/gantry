#!/usr/bin/env bash
set -euo pipefail

OLLAMA_URL="${OLLAMA_URL:-http://127.0.0.1:11434}"
MODEL="${MODEL:-qwen3.5:0.8b}"
MSG="${1:-Please just respond with hello}"

curl -sS "${OLLAMA_URL}/api/chat" \
	-H "Content-Type: application/json" \
	-d "{
    \"model\": \"${MODEL}\",
    \"messages\": [{\"role\":\"user\",\"content\":\"${MSG}\"}],
    \"think\": false,
    \"stream\": true
  }"
echo
