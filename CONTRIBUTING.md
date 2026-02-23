<!--
SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
SPDX-License-Identifier: Apache-2.0
-->

# How to contribute

Thank you for your interest in contributing!

## Eclipse Contributor Agreement

Before your contribution can be accepted by the project, you must electronically sign the
[Eclipse Contributor Agreement (ECA)](https://www.eclipse.org/legal/eca/).

To sign the ECA, visit <https://accounts.eclipse.org/user/eca> and complete the form.
You will need to create an Eclipse Foundation account if you don't already have one.

## Issues

Before contributing, please either:

* Pick an existing issue, e.g., those labeled [`good first issue`](https://github.com/eclipse-opensovd/opensovd-core/labels/good%20first%20issue)
* [Create a new issue](https://github.com/eclipse-opensovd/opensovd-core/issues/new) describing the work you plan to do

This helps coordinate efforts and ensures your contribution aligns with project goals.

## Conventions

This project follows these conventions:

* [GitHub Flow](https://docs.github.com/en/get-started/using-github/github-flow)
  * Create a feature branch
  * Make changes
  * Create a pull request
  * Handle code reviews
  * Merge pull request
  * An alternative name is `feature branch workflow`. Note: This is _not_ GitFlow.
* [Semantic Versioning](https://semver.org)
  * Version follows the schema MAJOR.MINOR.PATCH for release tags
  * Optionally a trailing pre-release/build identifier can be added e.g. 1.2.3-rc.1+01
* [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/)
  * Template commit message:

    ```text
    <type>[optional scope]: <description>

    [optional body]

    Issue: eclipse-opensovd/opensovd-core#<number>

    [optional footer(s)]
    ```

  * Reference the related issue in the footer
  * See git commit message [template](.gitmessage)
* Run [prek](https://prek.j178.dev/) before submitting a PR

## Legal considerations

* Newly created files contain a proper license header

  ```rust
  // SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
  // SPDX-License-Identifier: Apache-2.0

  ```
