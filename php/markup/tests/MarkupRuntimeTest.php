<?php

declare(strict_types=1);

namespace Markup\Tests;

use Markup\Element;
use Markup\Exception;
use Markup\Fragment;
use Markup\Html;
use PHPUnit\Framework\TestCase;

use function Markup\render;

/**
 * The RFC-parity runtime contract (docs/rfc-alignment.md: "Escaping is the
 * runtime's contract (escape-by-default)"): what `--target=rfc` emits is
 * `new \Markup\Element('tag', [...attrs], [...children])` with raw PHP
 * expressions riding through — so the runtime escapes every dynamic value
 * at print time under the same attribute routing the string target's
 * `\Lintel\a()/u()/cls()/sty()/srcset()` helpers enforce (doc §6.8, §7.1).
 */
final class MarkupRuntimeTest extends TestCase
{
    /** The lowering shape from the alignment table: tag, attrs, children. */
    public function test_element_renders_tag_attrs_children(): void
    {
        self::assertSame('<b class="x">hi</b>', (string) new Element('b', ['class' => 'x'], ['hi']));
        self::assertSame('<br>', (string) new Element('br'));
    }

    /** Escape-by-default: string children and scalar attribute values print escaped. */
    public function test_escapes_by_default(): void
    {
        $payload = '<script>alert("pwned")</script>';
        $el = new Element('p', ['title' => $payload], ['user: ', $payload]);
        self::assertSame(
            '<p title="&lt;script&gt;alert(&quot;pwned&quot;)&lt;/script&gt;">'
                .'user: &lt;script&gt;alert(&quot;pwned&quot;)&lt;/script&gt;</p>',
            $el->render(),
        );
    }

    /** The trusted sink: Markup\Html rides through verbatim (the RFC's \Markup\Html value type). */
    public function test_html_is_the_trusted_sink(): void
    {
        $el = new Element('div', [], [Html::unsafeRaw('<em>author-writ&shy;ten</em>')]);
        self::assertSame('<div><em>author-writ&shy;ten</em></div>', $el->render());
    }

    /** Fragments render their children in place (a root fragment lowers to a plain array — docs/rfc-alignment.md 4). */
    public function test_fragment_renders_children(): void
    {
        $frag = new Fragment(['a', new Element('b', [], ['c'])]);
        self::assertSame('a<b>c</b>', $frag->render());
    }

    /** Void elements render themselves (alignment table: `<br />` → `<br>`); children on a void are a bug. */
    public function test_void_elements_render_themselves_and_reject_children(): void
    {
        self::assertSame('<br>', new Element('br')->render());
        self::assertSame('<img src="x.png">', new Element('img', ['src' => 'x.png'])->render());
        $this->expectException(Exception::class);
        new Element('br', [], ['oops'])->render();
    }

    /** URL attributes scheme-check like \Lintel\u() (doc §4.4): hostile schemes become about:invalid#blocked. */
    public function test_url_attributes_scheme_check(): void
    {
        self::assertSame('<a href="about:invalid#blocked">x</a>', new Element('a', ['href' => 'javascript:alert(1)'], ['x'])->render());
        self::assertSame('<a href="about:invalid#blocked">x</a>', new Element('a', ['href' => "java\tscript:alert(1)"], ['x'])->render());
        self::assertSame('<a href="https://ok/">x</a>', new Element('a', ['href' => 'https://ok/'], ['x'])->render());
        self::assertSame('<a href="/relative">x</a>', new Element('a', ['href' => '/relative'], ['x'])->render());
        // The URL-attribute routing covers the emit.rs URL_ATTRS list.
        self::assertSame('<a ping="about:invalid#blocked"></a>', new Element('a', ['ping' => 'vbscript:x'])->render());
    }

    /** class={…} accepts string | list | map like \Lintel\cls(). */
    public function test_class_attribute_shapes(): void
    {
        self::assertSame('<b class="a b"></b>', new Element('b', ['class' => ['a', 'b']])->render());
        self::assertSame('<b class="on off"></b>', new Element('b', ['class' => ['on' => true, 'off' => 1, 'no' => false]])->render());
        self::assertSame('<b></b>', new Element('b', ['class' => ['']])->render());
    }

    /** style={…} map values ride the safe charset; style scalars print plain. */
    public function test_style_attribute_routing(): void
    {
        self::assertSame('<b style="color:red;width:10px"></b>', new Element('b', ['style' => ['color' => 'red', 'width' => '10px']])->render());
        self::assertSame('<b style="color:red"></b>', new Element('b', ['style' => 'color:red'])->render());
        $this->expectException(Exception::class);
        new Element('b', ['style' => ['color' => 'red;background:url(x)']])->render();
    }

    /** srcset splits candidates and scheme-checks each URL like \Lintel\srcset() (§7.1). */
    public function test_srcset_attribute_candidates(): void
    {
        self::assertSame(
            '<img srcset="a.png 1x, b.png 2x">',
            new Element('img', ['srcset' => 'a.png 1x, b.png 2x'])->render(),
        );
        self::assertSame(
            '<img srcset="about:invalid#blocked 2x">',
            new Element('img', ['srcset' => 'javascript:x 2x'])->render(),
        );
    }

    /** The attribute-name rules: on* handlers and invalid names are rejected, bool attrs render bare, falsy ride empty. */
    public function test_attribute_name_rules_and_bool_attrs(): void
    {
        self::assertSame('<b disabled></b>', new Element('b', ['disabled' => true])->render());
        self::assertSame('<b data-id="7"></b>', new Element('b', ['data-id' => 7])->render());
        self::assertSame('<b></b>', new Element('b', ['data-id' => null, 'gone' => false])->render());
        $this->expectException(Exception::class);
        new Element('b', ['onclick' => 'alert(1)'])->render();
    }

    /** Invalid attribute names are rejected like \Lintel\attrs(). */
    public function test_invalid_attribute_name_throws(): void
    {
        $this->expectException(Exception::class);
        new Element('b', ['bad name' => 'x'])->render();
    }

    /** Falsy guards render empty; scalars stringify; nested elements and iterables walk. */
    public function test_falsy_guards_scalars_and_iterables(): void
    {
        $el = new Element('div', [], [false, null, 0, 1.5, 'ok']);
        self::assertSame('<div>01.5ok</div>', $el->render());
        $nested = new Element('ul', [], [new Element('li', [], [1]), ['<em>', 'raw&data']]);
        self::assertSame('<ul><li>1</li>&lt;em&gt;raw&amp;data</ul>', $nested->render());
    }

    /** Attribute arrays spread ({...$props}) arrive as plain entries — same routing applies. */
    public function test_spread_entries_follow_the_same_routing(): void
    {
        $el = new Element('a', ['href' => 'javascript:x', 'data-x' => '1'], ['t']);
        self::assertSame('<a href="about:invalid#blocked" data-x="1">t</a>', $el->render());
    }

    /** Unknown child types are rejected (no silent stringification of objects). */
    public function test_unknown_child_type_throws(): void
    {
        $this->expectException(Exception::class);
        render(new Element('b', [], [new \stdClass]));
    }

    /** Invalid tag names are rejected (the compiler only emits real names; the runtime defends anyway). */
    public function test_invalid_tag_name_throws(): void
    {
        $this->expectException(Exception::class);
        new Element('<script')->render();
    }
}
