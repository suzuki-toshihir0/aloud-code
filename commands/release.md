Release aloud-code at version $ARGUMENTS.

Steps:
1. Confirm the version string (e.g. v0.1.7). If not provided or format is wrong, ask the user.
2. Strip the leading `v` to get the bare version number (e.g. 0.1.7).
3. Update `.claude-plugin/plugin.json` — replace the `"version"` field value with the bare version number.
4. `git add .claude-plugin/plugin.json` and commit with message `chore: bump plugin.json version to <bare-version>`.
5. `git push`.
6. `git tag -a v<bare-version> -m "v<bare-version>"` and `git push origin v<bare-version>`.
7. Report the tag that was pushed and the GitHub Actions URL to track the release build.
