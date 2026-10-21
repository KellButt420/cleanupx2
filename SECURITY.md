# Security

cleanup is a development-stage desktop utility. It runs with the current user's
permissions, coordinates official uninstallers, and moves only explicitly approved
files. There is no automatic cleanup or persistent privileged service.

For a suspected arbitrary command execution, incorrect deletion, path traversal,
link traversal, privilege issue or data-loss bug, contact the repository maintainer
privately. Use a GitHub private security advisory if that facility has been enabled
on the published repository. A public issue should contain only a sanitized symptom.
No private reporting endpoint has been configured in this local checkout.

Reproduce with disposable fixtures. Include the package version, Windows build,
minimal paths, expected outcome and actual result. Do not send credentials,
private certificates, personal inventories or unreviewed operation journals.

Only the current development branch receives fixes. There is no supported stable
release yet. An exported registry key or file journal does not guarantee rollback
of an uninstall or every interrupted operation.
