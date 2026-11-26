<?php

declare(strict_types=1);

namespace Lintl\Bench;

/**
 * Metric math and artifact writers for the §9.2 harness: percentile
 * aggregation over the 30 timed runs, one hand-rolled SVG bar chart per
 * scenario (no chart dependency), and the Markdown summary the methodology
 * page links to the raw JSON.
 */
final class Metrics
{
    /** @param array<int, float> $xs */
    public static function median(array $xs): float
    {
        \sort($xs);
        $n = \count($xs);
        if ($n === 0) {
            return 0.0;
        }
        $mid = intdiv($n, 2);

        return $n % 2 === 1 ? $xs[$mid] : ($xs[$mid - 1] + $xs[$mid]) / 2;
    }

    /** @param array<int, float> $xs */
    public static function percentile(array $xs, float $p): float
    {
        \sort($xs);
        $n = \count($xs);
        if ($n === 0) {
            return 0.0;
        }
        $idx = (int) \ceil($p / 100 * $n) - 1;
        $idx = \max(0, \min($n - 1, $idx));

        return $xs[$idx];
    }

    /**
     * Horizontal-bar SVG for one scenario: median ns/render per competitor.
     *
     * @param  array<string, float>  $medians
     */
    public static function barSvg(string $title, array $medians, string $unit): string
    {
        \arsort($medians);
        $max = \max($medians) ?: 1.0;
        $barH = 26;
        $gap = 10;
        $labelW = 150;
        $valueW = 130;
        $barMax = 560;
        $width = $labelW + $barMax + $valueW + 40;
        $height = \count($medians) * ($barH + $gap) + 70;
        $svg = '<?xml version="1.0" encoding="UTF-8"?>'."\n";
        $svg .= '<svg xmlns="http://www.w3.org/2000/svg" width="'.$width.'" height="'.$height.'" viewBox="0 0 '.$width.' '.$height.'" font-family="monospace" font-size="13">';
        $svg .= '<text x="20" y="26" font-size="15" font-weight="bold">'.\htmlspecialchars($title, \ENT_QUOTES | \ENT_SUBSTITUTE | \ENT_HTML5, 'UTF-8').'</text>';
        $y = 50;
        foreach ($medians as $name => $v) {
            $w = (int) \max(2, \round($v / $max * $barMax));
            $svg .= '<text x="20" y="'.($y + $barH / 2 + 4).'" text-anchor="start">'.\htmlspecialchars($name, \ENT_QUOTES | \ENT_SUBSTITUTE | \ENT_HTML5, 'UTF-8').'</text>';
            $svg .= '<rect x="'.($labelW + 10).'" y="'.$y.'" width="'.$w.'" height="'.($barH - 6).'" fill="#3a6ea5" rx="3"/>';
            $svg .= '<text x="'.($labelW + 10 + $w + 8).'" y="'.($y + $barH / 2 + 4).'">'.self::fmtNs($v).' '.$unit.'</text>';
            $y += $barH + $gap;
        }
        $svg .= '</svg>'."\n";

        return $svg;
    }

    public static function fmtNs(float $ns): string
    {
        return match (true) {
            $ns >= 1_000_000 => \sprintf('%.2f ms', $ns / 1_000_000),
            $ns >= 1_000 => \sprintf('%.2f µs', $ns / 1_000),
            default => \sprintf('%.0f ns', $ns),
        };
    }

    /** @param array<string, array<string, float>> $mediansBySetting setting ⇒ competitor ⇒ ns */
    public static function summaryTable(string $title, array $mediansBySetting, string $unit): string
    {
        $out = "### $title\n\n";
        $settings = \array_keys($mediansBySetting);
        $competitors = \array_keys($mediansBySetting[$settings[0]] ?? []);
        $out .= '| '.$unit.' (median) | '.\implode(' | ', $settings)." |\n";
        $out .= '|'.\str_repeat(' --- |', \count($settings) + 1)."\n";
        foreach ($competitors as $c) {
            $row = \array_map(static fn (string $s) => self::fmtNs($mediansBySetting[$s][$c] ?? 0.0), $settings);
            $out .= "| $c | ".\implode(' | ', $row)." |\n";
        }

        return $out."\n";
    }
}
