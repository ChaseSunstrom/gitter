#!/usr/bin/env python3
"""Generates static/css/themes.css: every skin x light/dark as CSS custom properties.

Run after changing a palette:  python3 scripts/gen-themes.py
A skin is chosen with <html data-skin="...">, the mode with data-theme="light|dark"
(absent = follow the system). Aurora is the default skin.
"""
from pathlib import Path

# Shared shape/type per skin.
SHAPE = {
    "aurora": dict(font_ui='"Geist", var(--sys)', font_display='"Bricolage Grotesque", "Geist", var(--sys)', font_mono='"Geist Mono", var(--sysmono)',
                   r="11px", r_sm="8px", btn_bw="1px", display_weight="760", display_track="-.025em"),
    "forge": dict(font_ui='"Onest", var(--sys)', font_display='"Unbounded", "Onest", var(--sys)', font_mono='"JetBrains Mono", var(--sysmono)',
                  r="10px", r_sm="7px", btn_bw="1px", display_weight="620", display_track="-.02em"),
    "ultraviolet": dict(font_ui='"IBM Plex Sans", var(--sys)', font_display='"JetBrains Mono", var(--sysmono)', font_mono='"JetBrains Mono", var(--sysmono)',
                        r="5px", r_sm="4px", btn_bw="1px", display_weight="700", display_track="-.035em"),
    "daylight": dict(font_ui='"Inter Tight", var(--sys)', font_display='"Bricolage Grotesque", "Inter Tight", var(--sys)', font_mono='"JetBrains Mono", var(--sysmono)',
                     r="12px", r_sm="9px", btn_bw="2px", display_weight="800", display_track="-.035em"),
}

# Colors: one dict per skin per mode.
C = {
    ("aurora", "dark"): dict(
        bg="#07080d", bg_sunk="#05060a", surface="#0e1019", surface_2="#131623", hover="#1a1e2f", line="#1c2032", line_strong="#2b3049",
        ink="#eceefe", ink_2="#c8cbe6", muted="#8b90ad", faint="#5f6482",
        accent="#a78bfa", accent_ink="#ffffff", accent_soft="rgb(124 92 255 / 14%)", accent_line="rgb(124 92 255 / 42%)", focus="#b69cff",
        brand_1="#7c5cff", brand_2="#e255d6", brand_3="#ff9a5a", logo_bg="#121528", logo_core="#fff4e6",
        grad="linear-gradient(110deg, #7c5cff, #e255d6 55%, #ff9a5a)", on_grad="#ffffff",
        glow="0 8px 26px -8px rgb(226 85 214 / 55%), 0 2px 8px -2px rgb(124 92 255 / 40%)",
        halo="radial-gradient(900px 280px at 8% -40%, rgb(124 92 255 / 22%), transparent 70%), radial-gradient(700px 240px at 92% -50%, rgb(255 154 90 / 12%), transparent 70%)",
        page_glow="radial-gradient(1200px 500px at 50% -260px, rgb(124 92 255 / 13%), transparent 70%)",
        btn_shadow="none", topbar="rgb(7 8 13 / 72%)",
        green="#3fd79a", green_soft="rgb(63 215 154 / 13%)", red="#ff6b8b", red_soft="rgb(255 107 139 / 13%)", purple="#b18cff", purple_soft="rgb(177 140 255 / 14%)",
        amber="#ffb86b", amber_soft="rgb(255 184 107 / 13%)", blue="#7fb2ff",
        add_bg="rgb(46 214 160 / 9%)", add_line="rgb(46 214 160 / 16%)", add_word="rgb(46 214 160 / 30%)",
        del_bg="rgb(255 90 130 / 9%)", del_line="rgb(255 90 130 / 16%)", del_word="rgb(255 90 130 / 32%)", hunk_bg="rgb(124 92 255 / 10%)",
        line_hl="rgb(124 92 255 / 14%)", line_hl_num="#7c5cff",
        hl_kw="#c39bff", hl_str="#ffb38a", hl_num="#ff9fd8", hl_fn="#7fd8ff", hl_title="#7fd8ff", hl_type="#ffd479", hl_com="#666b8c",
        hl_attr="#9fb4ff", hl_var="#ffcf99", hl_tag="#ff8ad8", hl_meta="#b69cff", hl_sym="#ff9fd8", hl_add="#5ef2c1", hl_del="#ff7d9c",
        shadow_sm="0 1px 2px rgb(0 0 0 / 40%)", shadow="0 1px 2px rgb(0 0 0 / 40%), 0 6px 20px rgb(0 0 0 / 32%)",
        shadow_lg="0 10px 30px rgb(0 0 0 / 50%), 0 30px 80px rgb(0 0 0 / 55%)", scrim="rgb(3 4 8 / 62%)", scheme="dark"),
    ("aurora", "light"): dict(
        bg="#f8f7ff", bg_sunk="#f1effc", surface="#ffffff", surface_2="#f7f5ff", hover="#efecfd", line="#e6e3f6", line_strong="#d3cdee",
        ink="#15142a", ink_2="#34325a", muted="#67668a", faint="#9a98b8",
        accent="#6d4dff", accent_ink="#ffffff", accent_soft="#efeaff", accent_line="#cfc2ff", focus="#6d4dff",
        brand_1="#6d4dff", brand_2="#d63fc6", brand_3="#ff8a45", logo_bg="#15142a", logo_core="#fff4e6",
        grad="linear-gradient(110deg, #6d4dff, #d63fc6 55%, #ff8a45)", on_grad="#ffffff",
        glow="0 8px 22px -10px rgb(214 63 198 / 55%), 0 2px 6px -2px rgb(109 77 255 / 35%)",
        halo="radial-gradient(900px 280px at 8% -40%, rgb(109 77 255 / 12%), transparent 70%), radial-gradient(700px 240px at 92% -50%, rgb(255 138 69 / 10%), transparent 70%)",
        page_glow="radial-gradient(1200px 500px at 50% -260px, rgb(109 77 255 / 8%), transparent 70%)",
        btn_shadow="none", topbar="rgb(248 247 255 / 78%)",
        green="#128a5c", green_soft="#dff6ec", red="#d6335c", red_soft="#fde6ec", purple="#7c3aed", purple_soft="#efe7fe",
        amber="#b45309", amber_soft="#fdf0dc", blue="#2563eb",
        add_bg="#e9f8f1", add_line="#cdeedd", add_word="#a6e2c4", del_bg="#fdecf0", del_line="#f8d2db", del_word="#f1a7b8", hunk_bg="#f1edff",
        line_hl="#f3efff", line_hl_num="#b9a7ff",
        hl_kw="#6d28d9", hl_str="#b4530a", hl_num="#c026d3", hl_fn="#0369a1", hl_title="#0369a1", hl_type="#a16207", hl_com="#8b8aa6",
        hl_attr="#4338ca", hl_var="#9a3412", hl_tag="#be185d", hl_meta="#7c3aed", hl_sym="#c026d3", hl_add="#15803d", hl_del="#be123c",
        shadow_sm="0 1px 2px rgb(21 20 42 / 6%)", shadow="0 1px 2px rgb(21 20 42 / 5%), 0 6px 18px rgb(21 20 42 / 7%)",
        shadow_lg="0 10px 30px rgb(21 20 42 / 12%), 0 30px 70px rgb(21 20 42 / 16%)", scrim="rgb(21 20 42 / 38%)", scheme="light"),

    ("forge", "dark"): dict(
        bg="#110d0b", bg_sunk="#0c0908", surface="#1a1411", surface_2="#211915", hover="#2a201b", line="#2e241e", line_strong="#45372e",
        ink="#f7ede5", ink_2="#dccbbf", muted="#a8968a", faint="#76675d",
        accent="#ff8a4c", accent_ink="#1a0c05", accent_soft="rgb(255 106 43 / 14%)", accent_line="rgb(255 106 43 / 42%)", focus="#ff9a5a",
        brand_1="#ffc04a", brand_2="#ff6a2b", brand_3="#e0245e", logo_bg="#1f1712", logo_core="#fff4d6",
        grad="linear-gradient(110deg, #ffc04a, #ff6a2b 55%, #e0245e)", on_grad="#1a0c05",
        glow="0 8px 24px -10px rgb(255 106 43 / 75%)",
        halo="radial-gradient(900px 280px at 92% -40%, rgb(255 106 43 / 16%), transparent 70%), radial-gradient(600px 220px at 10% -50%, rgb(255 192 74 / 9%), transparent 70%)",
        page_glow="radial-gradient(1200px 500px at 50% -260px, rgb(255 106 43 / 9%), transparent 70%)",
        btn_shadow="none", topbar="rgb(17 13 11 / 78%)",
        green="#a6e36a", green_soft="rgb(166 227 106 / 12%)", red="#ff6b5b", red_soft="rgb(255 107 91 / 13%)", purple="#e59bff", purple_soft="rgb(229 155 255 / 13%)",
        amber="#ffc04a", amber_soft="rgb(255 192 74 / 13%)", blue="#7ed4c6",
        add_bg="rgb(150 220 90 / 8%)", add_line="rgb(150 220 90 / 15%)", add_word="rgb(150 220 90 / 28%)",
        del_bg="rgb(255 90 70 / 9%)", del_line="rgb(255 90 70 / 16%)", del_word="rgb(255 90 70 / 30%)", hunk_bg="rgb(255 106 43 / 9%)",
        line_hl="rgb(255 192 74 / 12%)", line_hl_num="#ffc04a",
        hl_kw="#ff8f5a", hl_str="#f6d27a", hl_num="#ffb4a2", hl_fn="#7ed4c6", hl_title="#7ed4c6", hl_type="#ffc04a", hl_com="#7d6b60",
        hl_attr="#f0a37a", hl_var="#ffd8b0", hl_tag="#ff7a6b", hl_meta="#e0a8ff", hl_sym="#ffb4a2", hl_add="#a6e36a", hl_del="#ff7a6b",
        shadow_sm="0 1px 2px rgb(0 0 0 / 40%)", shadow="0 1px 2px rgb(0 0 0 / 40%), 0 6px 20px rgb(0 0 0 / 32%)",
        shadow_lg="0 10px 30px rgb(0 0 0 / 50%), 0 30px 80px rgb(0 0 0 / 55%)", scrim="rgb(8 5 3 / 62%)", scheme="dark"),
    ("forge", "light"): dict(
        bg="#fbf6f1", bg_sunk="#f5ece4", surface="#ffffff", surface_2="#fcf7f2", hover="#f5ebe2", line="#eddfd3", line_strong="#dcc8b8",
        ink="#241a14", ink_2="#47362b", muted="#7d6a5e", faint="#a8968a",
        accent="#d9481c", accent_ink="#ffffff", accent_soft="#fde8dd", accent_line="#f6c1a8", focus="#e8572a",
        brand_1="#f5a524", brand_2="#e8572a", brand_3="#c81e5a", logo_bg="#241a14", logo_core="#fff4d6",
        grad="linear-gradient(110deg, #f5a524, #e8572a 55%, #c81e5a)", on_grad="#ffffff",
        glow="0 8px 22px -10px rgb(232 87 42 / 60%)",
        halo="radial-gradient(900px 280px at 92% -40%, rgb(232 87 42 / 11%), transparent 70%), radial-gradient(600px 220px at 10% -50%, rgb(245 165 36 / 10%), transparent 70%)",
        page_glow="none", btn_shadow="none", topbar="rgb(251 246 241 / 80%)",
        green="#3f7d14", green_soft="#e7f3da", red="#c8321e", red_soft="#fbe3df", purple="#a3329b", purple_soft="#f6e3f4",
        amber="#b45309", amber_soft="#fdf0dc", blue="#0f766e",
        add_bg="#eef7e4", add_line="#d9edc4", add_word="#bfe19c", del_bg="#fcebe7", del_line="#f6d2ca", del_word="#ecab9c", hunk_bg="#fdf0e6",
        line_hl="#fff3d6", line_hl_num="#f5c766",
        hl_kw="#c2410c", hl_str="#9a5b00", hl_num="#be123c", hl_fn="#0f766e", hl_title="#0f766e", hl_type="#a16207", hl_com="#9b8a7e",
        hl_attr="#b45309", hl_var="#9a3412", hl_tag="#be123c", hl_meta="#a3329b", hl_sym="#be123c", hl_add="#3f7d14", hl_del="#c8321e",
        shadow_sm="0 1px 2px rgb(36 26 20 / 7%)", shadow="0 1px 2px rgb(36 26 20 / 6%), 0 6px 18px rgb(36 26 20 / 8%)",
        shadow_lg="0 10px 30px rgb(36 26 20 / 12%), 0 30px 70px rgb(36 26 20 / 16%)", scrim="rgb(36 26 20 / 38%)", scheme="light"),

    ("ultraviolet", "dark"): dict(
        bg="#0a0614", bg_sunk="#07040f", surface="#120b22", surface_2="#180f2d", hover="#21163c", line="#271a42", line_strong="#3c2a66",
        ink="#f1ebff", ink_2="#d4c9f2", muted="#9c8fc4", faint="#6a5e92",
        accent="#3ef0e0", accent_ink="#06121a", accent_soft="rgb(62 240 224 / 11%)", accent_line="rgb(62 240 224 / 42%)", focus="#3ef0e0",
        brand_1="#3ef0e0", brand_2="#8f7bff", brand_3="#ff4fd8", logo_bg="#160d2b", logo_core="#ffffff",
        grad="linear-gradient(90deg, #3ef0e0, #8f7bff 50%, #ff4fd8)", on_grad="#06121a",
        glow="0 0 22px -4px rgb(62 240 224 / 55%)",
        halo="linear-gradient(180deg, rgb(62 240 224 / 6%), transparent 70%), radial-gradient(700px 240px at 95% -40%, rgb(255 79 216 / 12%), transparent 70%)",
        page_glow="radial-gradient(1200px 500px at 50% -260px, rgb(143 123 255 / 12%), transparent 70%)",
        btn_shadow="none", topbar="rgb(10 6 20 / 78%)",
        green="#3ef0b0", green_soft="rgb(62 240 176 / 11%)", red="#ff4f8b", red_soft="rgb(255 79 139 / 12%)", purple="#ff4fd8", purple_soft="rgb(255 79 216 / 12%)",
        amber="#ffd84f", amber_soft="rgb(255 216 79 / 12%)", blue="#7aa2ff",
        add_bg="rgb(62 240 224 / 7%)", add_line="rgb(62 240 224 / 14%)", add_word="rgb(62 240 224 / 26%)",
        del_bg="rgb(255 79 216 / 8%)", del_line="rgb(255 79 216 / 15%)", del_word="rgb(255 79 216 / 28%)", hunk_bg="rgb(143 123 255 / 11%)",
        line_hl="rgb(62 240 224 / 10%)", line_hl_num="#3ef0e0",
        hl_kw="#ff79e1", hl_str="#9df7ef", hl_num="#ffd84f", hl_fn="#c7a6ff", hl_title="#c7a6ff", hl_type="#3ef0e0", hl_com="#6a5e92",
        hl_attr="#9df7ef", hl_var="#ffb0f0", hl_tag="#ff4fd8", hl_meta="#c7a6ff", hl_sym="#ffd84f", hl_add="#3ef0b0", hl_del="#ff4f8b",
        shadow_sm="0 1px 2px rgb(0 0 0 / 45%)", shadow="0 1px 2px rgb(0 0 0 / 45%), 0 6px 20px rgb(0 0 0 / 35%)",
        shadow_lg="0 10px 30px rgb(0 0 0 / 55%), 0 30px 80px rgb(0 0 0 / 60%)", scrim="rgb(4 2 10 / 66%)", scheme="dark"),
    ("ultraviolet", "light"): dict(
        bg="#f6f3ff", bg_sunk="#efeafd", surface="#ffffff", surface_2="#f5f1ff", hover="#ece5fb", line="#e3dcf7", line_strong="#cdc0ef",
        ink="#1a1033", ink_2="#3a2d63", muted="#6d6290", faint="#9a90b8",
        accent="#7a2cff", accent_ink="#ffffff", accent_soft="#efe6ff", accent_line="#d4bfff", focus="#7a2cff",
        brand_1="#00b8a9", brand_2="#7a2cff", brand_3="#d61fb0", logo_bg="#1a1033", logo_core="#ffffff",
        grad="linear-gradient(90deg, #00b8a9, #7a2cff 50%, #d61fb0)", on_grad="#ffffff",
        glow="0 6px 18px -8px rgb(122 44 255 / 55%)",
        halo="linear-gradient(180deg, rgb(0 184 169 / 6%), transparent 70%), radial-gradient(700px 240px at 95% -40%, rgb(214 31 176 / 8%), transparent 70%)",
        page_glow="none", btn_shadow="none", topbar="rgb(246 243 255 / 80%)",
        green="#0d8a6a", green_soft="#ddf5ee", red="#d61f5c", red_soft="#fde3ec", purple="#b5179e", purple_soft="#f9e1f4",
        amber="#a16207", amber_soft="#fbf0d6", blue="#4f46e5",
        add_bg="#e3f8f5", add_line="#c6efe9", add_word="#9de3d9", del_bg="#fde9f6", del_line="#f7cdea", del_word="#efa2d7", hunk_bg="#efe9ff",
        line_hl="#efe9ff", line_hl_num="#a87dff",
        hl_kw="#b5179e", hl_str="#0f766e", hl_num="#a16207", hl_fn="#6d28d9", hl_title="#6d28d9", hl_type="#0e7490", hl_com="#9a90b8",
        hl_attr="#0f766e", hl_var="#9d174d", hl_tag="#b5179e", hl_meta="#6d28d9", hl_sym="#a16207", hl_add="#0d8a6a", hl_del="#d61f5c",
        shadow_sm="0 1px 2px rgb(26 16 51 / 7%)", shadow="0 1px 2px rgb(26 16 51 / 6%), 0 6px 18px rgb(26 16 51 / 8%)",
        shadow_lg="0 10px 30px rgb(26 16 51 / 13%), 0 30px 70px rgb(26 16 51 / 17%)", scrim="rgb(26 16 51 / 40%)", scheme="light"),

    ("daylight", "light"): dict(
        bg="#ffffff", bg_sunk="#f4f5f7", surface="#ffffff", surface_2="#f6f7f9", hover="#eff1f5", line="#e2e4ea", line_strong="#0b0b0c",
        ink="#0b0b0c", ink_2="#26272c", muted="#5c5f69", faint="#8e919b",
        accent="#2f4bff", accent_ink="#ffffff", accent_soft="#e8ecff", accent_line="#b9c4ff", focus="#2f4bff",
        brand_1="#5b74ff", brand_2="#2f4bff", brand_3="#ffcf5c", logo_bg="#0b0b0c", logo_core="#ffffff",
        grad="#2f4bff", on_grad="#ffffff",
        glow="3px 3px 0 #0b0b0c",
        halo="linear-gradient(#ffffff, #ffffff)", page_glow="none",
        btn_shadow="2px 2px 0 #0b0b0c", topbar="rgb(255 255 255 / 86%)",
        green="#12a150", green_soft="#dcf6e6", red="#e5484d", red_soft="#fde4e4", purple="#8b5cf6", purple_soft="#efe8fe",
        amber="#c27c00", amber_soft="#fff4d6", blue="#2f4bff",
        add_bg="#e6f7ec", add_line="#c9edd5", add_word="#9fdfb4", del_bg="#fdebec", del_line="#f7cfd2", del_word="#f2a9af", hunk_bg="#eef1ff",
        line_hl="#fff6d6", line_hl_num="#ffcf5c",
        hl_kw="#2f4bff", hl_str="#a35200", hl_num="#c2185b", hl_fn="#7c3aed", hl_title="#7c3aed", hl_type="#0e7490", hl_com="#8e919b",
        hl_attr="#2f4bff", hl_var="#a35200", hl_tag="#12a150", hl_meta="#7c3aed", hl_sym="#c2185b", hl_add="#12a150", hl_del="#e5484d",
        shadow_sm="0 1px 2px rgb(11 11 12 / 7%)", shadow="0 1px 2px rgb(11 11 12 / 6%), 0 6px 18px rgb(11 11 12 / 8%)",
        shadow_lg="6px 6px 0 #0b0b0c", scrim="rgb(11 11 12 / 40%)", scheme="light"),
    ("daylight", "dark"): dict(
        bg="#0d0e12", bg_sunk="#090a0d", surface="#15171d", surface_2="#1b1e26", hover="#222631", line="#2a2e3a", line_strong="#f2f3f7",
        ink="#f2f3f7", ink_2="#d3d6df", muted="#9297a6", faint="#646979",
        accent="#7f93ff", accent_ink="#0b0d1a", accent_soft="rgb(111 132 255 / 15%)", accent_line="rgb(111 132 255 / 45%)", focus="#7f93ff",
        brand_1="#8a9bff", brand_2="#5b74ff", brand_3="#ffcf5c", logo_bg="#1b1e26", logo_core="#ffffff",
        grad="#5b74ff", on_grad="#ffffff",
        glow="3px 3px 0 #f2f3f7", halo="linear-gradient(#15171d, #15171d)", page_glow="none",
        btn_shadow="2px 2px 0 #f2f3f7", topbar="rgb(13 14 18 / 84%)",
        green="#4fd08a", green_soft="rgb(79 208 138 / 13%)", red="#ff6b6b", red_soft="rgb(255 107 107 / 13%)", purple="#b49bff", purple_soft="rgb(180 155 255 / 14%)",
        amber="#ffcf5c", amber_soft="rgb(255 207 92 / 13%)", blue="#7f93ff",
        add_bg="rgb(79 208 138 / 9%)", add_line="rgb(79 208 138 / 16%)", add_word="rgb(79 208 138 / 30%)",
        del_bg="rgb(255 107 107 / 9%)", del_line="rgb(255 107 107 / 16%)", del_word="rgb(255 107 107 / 30%)", hunk_bg="rgb(111 132 255 / 10%)",
        line_hl="rgb(255 207 92 / 10%)", line_hl_num="#ffcf5c",
        hl_kw="#8a9bff", hl_str="#ffcf5c", hl_num="#ff8fb1", hl_fn="#4fd0c8", hl_title="#4fd0c8", hl_type="#b49bff", hl_com="#646979",
        hl_attr="#8a9bff", hl_var="#ffcf5c", hl_tag="#4fd08a", hl_meta="#b49bff", hl_sym="#ff8fb1", hl_add="#4fd08a", hl_del="#ff6b6b",
        shadow_sm="0 1px 2px rgb(0 0 0 / 40%)", shadow="0 1px 2px rgb(0 0 0 / 40%), 0 6px 20px rgb(0 0 0 / 30%)",
        shadow_lg="6px 6px 0 #f2f3f7", scrim="rgb(0 0 0 / 60%)", scheme="dark"),
}

DEFAULT = "aurora"


def block(sel, d):
    lines = [f"  --{k.replace('_', '-')}: {v};" for k, v in d.items() if k != "scheme"]
    lines.append(f"  color-scheme: {d['scheme']};")
    return f"{sel} {{\n" + "\n".join(lines) + "\n}\n"


def sel(skin, extra=""):
    s = f':root[data-skin="{skin}"]{extra}'
    return f":root:not([data-skin]){extra}, {s}" if skin == DEFAULT else s


out = [__doc__.replace('"""', "").strip().replace("\n", "\n * ").join(["/* ", " */\n"]),
       ":root { --sys: ui-sans-serif, system-ui, -apple-system, \"Segoe UI\", Roboto, sans-serif; --sysmono: ui-monospace, \"SF Mono\", Menlo, Consolas, monospace; }\n"]
for skin, shape in SHAPE.items():
    out.append(block(sel(skin), {**shape, **C[(skin, "light")]}))
    out.append("@media (prefers-color-scheme: dark) {\n" + block(sel(skin, ':not([data-theme="light"])'), C[(skin, "dark")]) + "}\n")
    out.append(block(sel(skin, '[data-theme="dark"]'), C[(skin, "dark")]))

FONTS = [("Geist", "geist.woff2", "100 900"), ("Geist Mono", "geist-mono.woff2", "100 900"),
         ("Bricolage Grotesque", "bricolage-grotesque.woff2", "200 800"), ("Unbounded", "unbounded.woff2", "200 900"),
         ("Onest", "onest.woff2", "100 900"), ("JetBrains Mono", "jetbrains-mono.woff2", "100 800"),
         ("Inter Tight", "inter-tight.woff2", "100 900"), ("IBM Plex Sans", "ibm-plex-sans-400.woff2", "400"),
         ("IBM Plex Sans", "ibm-plex-sans-500.woff2", "500"), ("IBM Plex Sans", "ibm-plex-sans-600.woff2", "600 700")]
for fam, file, w in FONTS:
    out.append(f'@font-face {{ font-family: "{fam}"; src: url("../fonts/{file}") format("woff2"); font-weight: {w}; font-style: normal; font-display: swap; }}\n')

Path(__file__).resolve().parent.parent.joinpath("static/css/themes.css").write_text("\n".join(out))
print("wrote static/css/themes.css")
