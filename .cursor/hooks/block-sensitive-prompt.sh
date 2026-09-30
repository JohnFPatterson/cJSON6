#!/usr/bin/env bash
# Block prompt submission when the text contains PII or a plaintext API key.
# Prints Cursor beforeSubmitPrompt JSON. Never echoes the matched value.
set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  printf 'block-sensitive-prompt.sh requires jq on PATH\n' >&2
  exit 1
fi

payload="$(cat)"
if [[ -z "${payload//[[:space:]]/}" ]]; then
  payload="{}"
fi

prompt="$(jq -r '.prompt // ""' <<<"$payload")"
prompt_lower="$(printf '%s' "$prompt" | tr '[:upper:]' '[:lower:]')"

match_count=0

add_match() {
  local name="$1"
  local i
  for ((i = 0; i < match_count; i++)); do
    if [[ "${matches[$i]}" == "$name" ]]; then
      return 0
    fi
  done
  matches[$match_count]="$name"
  match_count=$((match_count + 1))
}

luhn_valid() {
  local digits="$1"
  local len=${#digits}
  local i d sum=0 double=0

  # An all-zero run passes Luhn and is not a card number.
  [[ "$digits" == *[!0]* ]] || return 1

  for ((i = len - 1; i >= 0; i--)); do
    d=$((10#${digits:i:1}))
    if ((double)); then
      d=$((d * 2))
      if ((d > 9)); then
        d=$((d - 9))
      fi
    fi
    sum=$((sum + d))
    double=$((1 - double))
  done
  ((sum % 10 == 0))
}

is_nonsensitive_assignment() {
  local value="$1"
  local lower
  lower="$(printf '%s' "$value" | tr '[:upper:]' '[:lower:]')"

  [[ "$value" == \$* ]] && return 0
  [[ "$lower" == *process.env* || "$lower" == *os.environ* ]] && return 0
  [[ "$lower" == *your_api_key* || "$lower" == *your-api-key* ]] && return 0
  [[ "$lower" == *redacted* ]] && return 0
  [[ "$value" == *"***"* ]] && return 0
  [[ "$lower" == \<*\> ]] && return 0
  return 1
}

scan_cards() {
  local candidate digits len
  while IFS= read -r candidate; do
    [[ -z "$candidate" ]] && continue
    digits="$(printf '%s' "$candidate" | tr -cd '0-9')"
    len=${#digits}
    if ((len >= 13 && len <= 19)) && luhn_valid "$digits"; then
      add_match "credit card-like number"
      return 0
    fi
  done < <(printf '%s\n' "$prompt" | grep -oE '[0-9]([ -]*[0-9])*' || true)
  return 0
}

scan_assignments() {
  local line value
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    value="$(printf '%s' "$line" | sed -E 's/^[^:=]*[:=][[:space:]]*//; s/^"//; s/"$//; s/^'\''//; s/'\''$//; s/[,.;]+$//')"
    if is_nonsensitive_assignment "$value"; then
      continue
    fi
    if ((${#value} >= 16)) && [[ "$value" =~ $raw_secret_re ]]; then
      add_match "API key"
      return 0
    fi
  done < <(printf '%s\n' "$prompt" | grep -oE -i '(api[_-]?key|apikey|secret|token)[[:space:]]*[:=][[:space:]]*[^[:space:]]+' || true)
  return 0
}

# Bash 3.2 misparses regular expressions written inline in [[ =~ ]].
raw_secret_re='^[A-Za-z0-9_./+=-]{16,}$'
email_re='(^|[^[:alnum:]_.%+-])[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}([^[:alnum:]]|$)'
ssn_re='(^|[^0-9])[0-9]{3}-[0-9]{2}-[0-9]{4}([^0-9]|$)'
phone_re='(^|[^0-9])(\+1[-. ]?)?\(?[0-9]{3}\)?[-. ][0-9]{3}[-. ][0-9]{4}([^0-9]|$)'
private_key_re='-----begin (rsa |dsa |ec |openssh |pgp )?private key-----'
cursor_key_re='(^|[^[:alnum:]_-])crsr_[A-Za-z0-9_-]{20,}([^[:alnum:]_-]|$)'
github_token_re='(^|[^[:alnum:]_])(ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9_]{30,}([^[:alnum:]_]|$)'
github_pat_re='(^|[^[:alnum:]_])github_pat_[A-Za-z0-9_]{22,}([^[:alnum:]_]|$)'
slack_token_re='(^|[^[:alnum:]_-])xox[baprs]-[A-Za-z0-9-]{20,}([^[:alnum:]_-]|$)'
anthropic_key_re='(^|[^[:alnum:]_-])sk-ant-[A-Za-z0-9_-]{20,}([^[:alnum:]_-]|$)'
openai_key_re='(^|[^[:alnum:]_-])sk-[A-Za-z0-9_-]{32,}([^[:alnum:]_-]|$)'
aws_key_re='(^|[^[:alnum:]_])(AKIA|ASIA)[A-Z0-9]{16}([^[:alnum:]_]|$)'
jwt_re='(^|[^[:alnum:]_-])eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}([^[:alnum:]_-]|$)'

if [[ "$prompt" =~ $email_re ]]; then
  add_match "email address"
fi

if [[ "$prompt" =~ $ssn_re ]]; then
  add_match "SSN"
fi

if [[ "$prompt" =~ $phone_re ]]; then
  add_match "phone number"
fi

scan_cards

if [[ "$prompt_lower" =~ $private_key_re ]]; then
  add_match "private key"
fi

if [[ "$prompt" =~ $cursor_key_re ]]; then
  add_match "Cursor API key"
fi

if [[ "$prompt" =~ $github_token_re ]]; then
  add_match "GitHub token"
fi

if [[ "$prompt" =~ $github_pat_re ]]; then
  add_match "GitHub token"
fi

if [[ "$prompt" =~ $slack_token_re ]]; then
  add_match "Slack token"
fi

if [[ "$prompt" =~ $anthropic_key_re ]]; then
  add_match "Anthropic API key"
fi

without_anthropic="$(printf '%s' "$prompt" | sed -E 's/sk-ant-[A-Za-z0-9_-]+/ /g')"
if [[ "$without_anthropic" =~ $openai_key_re ]]; then
  add_match "OpenAI API key"
fi

if [[ "$prompt" =~ $aws_key_re ]]; then
  add_match "AWS access key ID"
fi

if [[ "$prompt" =~ $jwt_re ]]; then
  add_match "JWT-like token"
fi

scan_assignments

if ((match_count > 0)); then
  match_list="${matches[0]}"
  for ((i = 1; i < match_count; i++)); do
    match_list+=", ${matches[$i]}"
  done
  message="This prompt appears to contain sensitive data (${match_list}). Remove or replace the sensitive value before resubmitting."
  jq -cn --arg message "$message" '{"continue":false,"user_message":$message}'
else
  printf '{"continue":true}\n'
fi
