# GitHub Labels

> Label taxonomy for issues and pull requests.

Labels are organized into two dimensions, each with a single-letter prefix so they sort naturally in the GitHub UI.

## Rules

- `T-*` issue type. EXACTLY ONE REQUIRED.
- `A-*` issue area. OPTIONAL. MULTIPLE ALLOWED.

## Labels

### Area (A)

| Label          | Description                            |
| -------------- | -------------------------------------- |
| `A-consensus`  | Consensus mechanism (Generally)        |
| `A-pallet`     | Runtime pallet                         |
| `A-session`    | Session management                     |
| `A-validator`  | Validator system                       |
| `A-difficulty` | Difficulty adjustment system           |
| `A-pow`        | POW block production, mining algorithm |
| `A-grandpa`    | GRANDPA finality and fork-choice       |
| `A-evm`        | Frontier EVM integration               |
| `A-node`       | Node binary, service layer, CLI        |
| `A-runtime`    | Runtime configuration and framework    |
| `A-imonline`   | im-online                              |
| `A-governance` | OpenGov, treasury and bounties         |
| `A-prime/sudo` | Privileged account with root access    |

### Type (T)

| Label             | Description                                                 |
| ----------------- | ----------------------------------------------------------- |
| `T-enhancement`   | New feature or request                                      |
| `T-bug`           | Something isn't working                                     |
| `T-refactor`      | Refactoring / code improvement                              |
| `T-test`          | Test cases                                                  |
| `T-documentation` | Improvements or additions to documentation                  |
| `T-ci`            | CI/CD, build, Dockerfile                                    |
| `T-discussion`    | Discussion / Further information is requested / Help wanted |
