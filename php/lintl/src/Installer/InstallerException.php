<?php

declare(strict_types=1);

namespace Lintl\Installer;

/** Refusal shapes for the §12 installer: bad downloads, checksum mismatches, unusable destinations. */
final class InstallerException extends \RuntimeException {}
