<?php

declare(strict_types=1);

namespace Lintel\Installer;

use Composer\Composer;
use Composer\EventDispatcher\EventSubscriberInterface;
use Composer\IO\IOInterface;
use Composer\Plugin\PluginInterface;
use Composer\Script\Event;
use Composer\Script\ScriptEvents;

/**
 * The §12 composer plugin: on every `post-autoload-dump` it makes sure
 * `<vendor>/bin/lintel` exists — from the LINTEL_BIN override or from the
 * pinned `extra.lintel` source + SHA-256 (BinaryInstaller::resolve owns the
 * precedence, the verification and the no-invented-URL rule). Prod never
 * needs the binary: it runs the compiled output plus lintel/runtime.
 */
final class Plugin implements PluginInterface, EventSubscriberInterface
{
    public function activate(Composer $composer, IOInterface $io): void
    {
        // State-free: every dump event re-resolves from the environment and
        // composer.json, so LINTEL_BIN changes apply on the next dump.
    }

    public function deactivate(Composer $composer, IOInterface $io): void
    {
        // Nothing to unwind: the plugin holds no state (see activate()).
    }

    public function uninstall(Composer $composer, IOInterface $io): void
    {
        // vendor/bin/lintel is a cache, not a plugin artifact (§12) — the
        // next autoload dump re-installs it, so nothing is cleaned up here.
    }

    /**
     * @return array<string, string>
     */
    public static function getSubscribedEvents(): array
    {
        return [ScriptEvents::POST_AUTOLOAD_DUMP => 'installBinary'];
    }

    public function installBinary(Event $event): void
    {
        $composer = $event->getComposer();
        $vendorDir = $composer->getConfig()->get('vendor-dir');
        BinaryInstaller::resolve(
            $composer->getPackage()->getExtra(),
            (\is_string($vendorDir) && $vendorDir !== '' ? $vendorDir : 'vendor') . '/bin',
            $event->getIo(),
        );
    }
}
