Release aloud-code at version $ARGUMENTS.

Steps:
1. Determine the version:
   - If $ARGUMENTS is a valid version string (e.g. `v0.1.7` or `0.1.7`), use it.
   - Otherwise, get the latest tag with `git tag --sort=-v:refname | head -1` to find the current version, then use AskUserQuestion to ask whether this is a major, minor, or patch update. Calculate the next version accordingly (e.g. patch: 0.1.5 → 0.1.6).
2. Strip the leading `v` to get the bare version number (e.g. 0.1.7).
3. Update `.claude-plugin/plugin.json` — replace the `"version"` field value with the bare version number.
4. `git add .claude-plugin/plugin.json` and commit with message `chore: bump plugin.json version to <bare-version>`.
5. `git push`.
6. `git tag -a v<bare-version> -m "v<bare-version>"` and `git push origin v<bare-version>`.
7. Report the tag that was pushed and the GitHub Actions URL to track the release build.
