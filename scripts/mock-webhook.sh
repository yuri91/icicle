#!/usr/bin/env bash

# Mock GitHub webhook script for testing icicle
# Usage: ./scripts/mock-webhook.sh [options]

set -euo pipefail

# Default values
REPO="test-org/test-repo"
COMMIT="abc123def456789"
BRANCH="main"
EVENT_TYPE="push"
PR_NUMBER="42"
PR_ACTION="opened"
SERVER_URL="http://localhost:3000"
SECRET=""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

show_help() {
    cat << EOF
Mock GitHub Webhook Script

Usage: $0 [OPTIONS]

OPTIONS:
    -r, --repo REPO         Repository name (default: test-org/test-repo)
    -c, --commit COMMIT     Commit SHA (default: abc123def456789)
    -b, --branch BRANCH     Branch name (default: main)
    -e, --event EVENT       Event type: push, pull_request, star (default: push)
    -n, --pr-number NUMBER  Pull request number (default: 42)
    -a, --pr-action ACTION  PR action: opened, synchronize, reopened, closed (default: opened)
    -u, --url URL           Server URL (default: http://localhost:3000)
    -s, --secret SECRET     Webhook secret for signature generation
    -h, --help              Show this help

EXAMPLES:
    # Basic push event
    $0

    # Push to specific repo and branch
    $0 -r myorg/myrepo -b feature/cool-stuff -c def456abc789

    # Pull request event
    $0 -e pull_request -n 123 -a synchronize

    # Test ignored event
    $0 -e star

    # With webhook secret
    $0 -s "my-webhook-secret"

EOF
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -r|--repo)
            REPO="$2"
            shift 2
            ;;
        -c|--commit)
            COMMIT="$2"
            shift 2
            ;;
        -b|--branch)
            BRANCH="$2"
            shift 2
            ;;
        -e|--event)
            EVENT_TYPE="$2"
            shift 2
            ;;
        -n|--pr-number)
            PR_NUMBER="$2"
            shift 2
            ;;
        -a|--pr-action)
            PR_ACTION="$2"
            shift 2
            ;;
        -u|--url)
            SERVER_URL="$2"
            shift 2
            ;;
        -s|--secret)
            SECRET="$2"
            shift 2
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            show_help
            exit 1
            ;;
    esac
done

# Generate timestamp
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Extract repo parts
REPO_NAME=$(basename "$REPO")
REPO_OWNER=$(dirname "$REPO")

# Generate payload based on event type
generate_payload() {
    case "$EVENT_TYPE" in
        "push")
            cat << EOF
{
  "ref": "refs/heads/$BRANCH",
  "after": "$COMMIT",
  "before": "0000000000000000000000000000000000000000",
  "created": false,
  "deleted": false,
  "forced": false,
  "base_ref": null,
  "compare": "https://github.com/$REPO/compare/0000000...$COMMIT",
  "commits": [
    {
      "id": "$COMMIT",
      "tree_id": "tree123",
      "distinct": true,
      "message": "Mock commit for testing",
      "timestamp": "$TIMESTAMP",
      "url": "https://github.com/$REPO/commit/$COMMIT",
      "author": {
        "name": "Test User",
        "email": "test@example.com",
        "username": "testuser"
      },
      "committer": {
        "name": "Test User", 
        "email": "test@example.com",
        "username": "testuser"
      },
      "added": ["new-file.txt"],
      "removed": [],
      "modified": ["existing-file.txt"]
    }
  ],
  "head_commit": {
    "id": "$COMMIT",
    "tree_id": "tree123",
    "distinct": true,
    "message": "Mock commit for testing",
    "timestamp": "$TIMESTAMP",
    "url": "https://github.com/$REPO/commit/$COMMIT",
    "author": {
      "name": "Test User",
      "email": "test@example.com",
      "username": "testuser"
    },
    "committer": {
      "name": "Test User",
      "email": "test@example.com", 
      "username": "testuser"
    },
    "added": ["new-file.txt"],
    "removed": [],
    "modified": ["existing-file.txt"]
  },
  "repository": {
    "id": 123456789,
    "name": "$REPO_NAME",
    "full_name": "$REPO",
    "private": false,
    "owner": {
      "name": "$REPO_OWNER",
      "login": "$REPO_OWNER"
    },
    "html_url": "https://github.com/$REPO",
    "description": "Test repository for icicle CI",
    "clone_url": "https://github.com/$REPO.git",
    "ssh_url": "git@github.com:$REPO.git",
    "default_branch": "main",
    "created_at": "2023-01-01T00:00:00Z",
    "updated_at": "$TIMESTAMP"
  },
  "pusher": {
    "name": "testuser",
    "email": "test@example.com"
  },
  "sender": {
    "login": "testuser",
    "id": 12345,
    "type": "User"
  }
}
EOF
            ;;
        "pull_request")
            cat << EOF
{
  "action": "$PR_ACTION",
  "number": $PR_NUMBER,
  "pull_request": {
    "id": 987654321,
    "number": $PR_NUMBER,
    "state": "open",
    "locked": false,
    "title": "Mock PR for testing",
    "user": {
      "login": "testuser",
      "id": 12345,
      "type": "User"
    },
    "body": "This is a mock pull request for testing icicle CI",
    "created_at": "$TIMESTAMP",
    "updated_at": "$TIMESTAMP",
    "head": {
      "label": "$REPO_OWNER:feature-branch",
      "ref": "feature-branch",
      "sha": "$COMMIT",
      "user": {
        "login": "$REPO_OWNER",
        "id": 67890,
        "type": "User"
      },
      "repo": {
        "id": 123456789,
        "name": "$REPO_NAME",
        "full_name": "$REPO",
        "private": false,
        "clone_url": "https://github.com/$REPO.git",
        "ssh_url": "git@github.com:$REPO.git"
      }
    },
    "base": {
      "label": "$REPO_OWNER:$BRANCH",
      "ref": "$BRANCH", 
      "sha": "base123abc456",
      "user": {
        "login": "$REPO_OWNER",
        "id": 67890,
        "type": "User"
      },
      "repo": {
        "id": 123456789,
        "name": "$REPO_NAME",
        "full_name": "$REPO",
        "private": false,
        "clone_url": "https://github.com/$REPO.git",
        "ssh_url": "git@github.com:$REPO.git"
      }
    }
  },
  "repository": {
    "id": 123456789,
    "name": "$REPO_NAME",
    "full_name": "$REPO",
    "private": false,
    "owner": {
      "login": "$REPO_OWNER",
      "id": 67890,
      "type": "User"
    },
    "html_url": "https://github.com/$REPO",
    "description": "Test repository for icicle CI",
    "clone_url": "https://github.com/$REPO.git",
    "ssh_url": "git@github.com:$REPO.git",
    "default_branch": "main"
  },
  "sender": {
    "login": "testuser",
    "id": 12345,
    "type": "User"
  }
}
EOF
            ;;
        *)
            # Generic event for testing ignored events
            cat << EOF
{
  "action": "created",
  "repository": {
    "id": 123456789,
    "name": "$REPO_NAME", 
    "full_name": "$REPO",
    "private": false,
    "clone_url": "https://github.com/$REPO.git",
    "ssh_url": "git@github.com:$REPO.git"
  },
  "sender": {
    "login": "testuser",
    "id": 12345,
    "type": "User"
  }
}
EOF
            ;;
    esac
}

# Generate webhook signature if secret is provided
generate_signature() {
    local payload="$1"
    local secret="$2"
    
    if command -v openssl >/dev/null 2>&1; then
        echo -n "$payload" | openssl dgst -sha256 -hmac "$secret" | sed 's/^.* //'
    else
        echo -e "${YELLOW}Warning: openssl not found, cannot generate signature${NC}" >&2
        echo ""
    fi
}

# Main execution
main() {
    echo -e "${GREEN}Sending mock $EVENT_TYPE webhook...${NC}"
    echo "Repository: $REPO"
    echo "Commit: $COMMIT"
    
    if [[ "$EVENT_TYPE" == "push" ]]; then
        echo "Branch: $BRANCH"
    elif [[ "$EVENT_TYPE" == "pull_request" ]]; then
        echo "PR Number: $PR_NUMBER"
        echo "PR Action: $PR_ACTION"
    fi
    
    echo "Server: $SERVER_URL"
    echo

    # Generate payload
    PAYLOAD=$(generate_payload)
    
    # Prepare curl command
    CURL_ARGS=(
        -X POST
        "$SERVER_URL/webhook/github"
        -H "Content-Type: application/json"
        -H "X-GitHub-Event: $EVENT_TYPE"
        -H "X-GitHub-Delivery: $(uuidgen 2>/dev/null || echo "test-delivery-$(date +%s)")"
        -H "User-Agent: GitHub-Hookshot/test"
        -d "$PAYLOAD"
        -w "\n\nHTTP Status: %{http_code}\nResponse Time: %{time_total}s\n"
    )
    
    # Add signature header if secret is provided
    if [[ -n "$SECRET" ]]; then
        SIGNATURE=$(generate_signature "$PAYLOAD" "$SECRET")
        if [[ -n "$SIGNATURE" ]]; then
            CURL_ARGS+=(-H "X-Hub-Signature-256: sha256=$SIGNATURE")
            echo -e "${GREEN}Using webhook secret for signature verification${NC}"
        fi
    fi
    
    # Execute the request
    echo -e "${YELLOW}Executing webhook request...${NC}"
    echo
    
    if curl "${CURL_ARGS[@]}"; then
        echo -e "\n${GREEN}✓ Webhook sent successfully${NC}"
    else
        echo -e "\n${RED}✗ Failed to send webhook${NC}"
        exit 1
    fi
}

# Check if curl is available
if ! command -v curl >/dev/null 2>&1; then
    echo -e "${RED}Error: curl is required but not installed${NC}"
    exit 1
fi

main