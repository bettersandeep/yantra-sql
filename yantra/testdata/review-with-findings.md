<h3>Code Review by Qodo</h3>

<br/>

<img src="https://img.shields.io/badge/High-634FD1?style=flat-square" height="20px" alt="Action required">

<details>
<summary>  1.  Changed test data can pass CI <code>🐞 Bug</code></summary>

<br/>

> <details>
><summary>Description</summary>
><br/>
>
><pre>
>The checksum step compares each test file only with <b><i>SHA256SUMS</i></b> from the same checkout, so a pull
>request can change both together and pass. The harness then measures the altered records, allowing a
>test-data change to affect the score without being rejected by this check.
></pre>
></details>

> <details>
><summary>Agent prompt</summary>
><br/>
>
>```
>The issue below was found during a code review. Follow the provided context and guidance below and implement a solution
>
>## Issue description
>A pull request can alter a test file and its checksum together, bypassing the new test-data safeguard.
>
>## Fix Focus Areas
>- .github/workflows/ci.yml[15-19]
>- tests/slt/SHA256SUMS[1-1]
>
>## Recommended Fix
>Validate test-file hashes against a source the same pull request cannot update to match its changes, or explicitly reject test-file and checksum-manifest changes in engine pull requests. Keep checksum verification for accidental corruption as a separate check.
>```
> <code>ⓘ Copy this prompt and use it to remediate the issue with your preferred AI generation tools</code>
></details>

[Dismiss ↗](https://app.qodo.ai/findings/4cf811d3-1141-437f-88b3-6e65406617a7?action=dismiss&origin=git) | [View ↗](https://app.qodo.ai/findings/4cf811d3-1141-437f-88b3-6e65406617a7)

</details>


<br/>

<details><summary><ins><strong>View medium (2)</strong></ins></summary><br/>
<img src="https://img.shields.io/badge/Medium-634FD1?style=flat-square" height="20px" alt="Remediation recommended">

<details>
<summary>  2.  Unlisted test files can pass CI <code>🐞 Bug</code></summary>

<br/>

> <details>
><summary>Description</summary>
><br/>
>
><pre>
>The completeness step compares the number of <b><i>*.test</i></b> files with the number of manifest lines, but
>never compares their paths. With the current single test, replacing its manifest entry with a valid
>checksum for <b><i>FORMAT.wiki</i></b> preserves both counts and passes <b><i>sha256sum -c</i></b> while leaving the test
>unlisted.
></pre>
></details>

> <details>
><summary>Agent prompt</summary>
><br/>
>
>```
>The issue below was found during a code review. Follow the provided context and guidance below and implement a solution
>
>## Issue description
>Equal file and manifest counts do not establish that every test has a checksum entry.
>
>## Fix Focus Areas
>- .github/workflows/ci.yml[15-19]
>
>## Recommended Fix
>Extract and normalize the paths in `SHA256SUMS`, compare them as a set with the recursively discovered `*.test` paths, and reject missing, extra, or duplicate entries before running `sha256sum -c`.
>```
> <code>ⓘ Copy this prompt and use it to remediate the issue with your preferred AI generation tools</code>
></details>

[Dismiss ↗](https://app.qodo.ai/findings/a16903d3-3f53-4026-9c86-b2b950ef7989?action=dismiss&origin=git) | [View ↗](https://app.qodo.ai/findings/a16903d3-3f53-4026-9c86-b2b950ef7989)

</details>


<details>
<summary>  3.  Dependencies can enter unnoticed <code>🐞 Bug</code></summary>

<br/>

> <details>
><summary>Description</summary>
><br/>
>
><pre>
>The dependency check counts packages in the committed <b><i>Cargo.lock</i></b> without checking whether it
>reflects <b><i>Cargo.toml</i></b>. If a pull request adds a dependency only to the manifest, this step still
>passes and the subsequent Cargo commands can resolve and build that dependency because they do not
>use <b><i>--locked</i></b>.
></pre>
></details>

> <details>
><summary>Agent prompt</summary>
><br/>
>
>```
>The issue below was found during a code review. Follow the provided context and guidance below and implement a solution
>
>## Issue description
>A stale one-package lockfile lets a new dependency evade the lockfile-only check.
>
>## Fix Focus Areas
>- .github/workflows/ci.yml[20-24]
>- Cargo.toml[1-6]
>
>## Recommended Fix
>Check dependency declarations in `Cargo.toml` as well as the lockfile, including target and development dependencies, and run subsequent Cargo build commands with `--locked` so they fail rather than updating a stale lockfile.
>```
> <code>ⓘ Copy this prompt and use it to remediate the issue with your preferred AI generation tools</code>
></details>

[Dismiss ↗](https://app.qodo.ai/findings/f3fc4c75-8b07-4018-b490-ae860f1d1153?action=dismiss&origin=git) | [View ↗](https://app.qodo.ai/findings/f3fc4c75-8b07-4018-b490-ae860f1d1153)

</details>


</details>

<br/>

<!-- qodo-self-review-slot -->
<!-- https://github.com/bettersandeep/lekha/commit/ee116d9c813d87fd7417ae080327d228b67acc33 -->
