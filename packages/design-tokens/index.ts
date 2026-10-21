/** Frozen visual vocabulary from the Figma baseline. Keep product semantics out of this package. */
export const color = {
  canvas: "#F5F6F8",
  surface: "#FFFFFF",
  ink: "#20242D",
  muted: "#626978",
  line: "#E1E5EC",
  brand: "#465BD8",
  /**
   * 禁用态的底色与文字色。禁用控件仍要能被读清，所以这里不用 opacity 淡化，
   * 而是给一组自带 4.5:1 对比度的实色（见 tokens.test.ts 的对比度断言）。
   */
  disabled: {
    surface: "#E9ECF1",
    ink: "#525A6B",
    /** 禁用态的品牌底色（主按钮禁用时用），同样满足 4.5:1。 */
    brandSurface: "#DDE2F7",
  },
  status: {
    info: "#3653C7",
    infoSurface: "#EEF1FF",
    warning: "#8A5A00",
    warningSurface: "#FFF5D6",
    success: "#237A4B",
    successSurface: "#EAF7EF",
    danger: "#B42318",
    dangerSurface: "#FEECEA",
  },
} as const;

export const spacing = {
  0: "0px",
  1: "4px",
  2: "8px",
  3: "12px",
  4: "16px",
  5: "20px",
  6: "24px",
  8: "32px",
} as const;

export const radius = {
  control: "8px",
  panel: "12px",
  pill: "999px",
} as const;

export const typography = {
  fontFamily: '"Noto Sans SC", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif',
  pageTitle: { fontSize: "28px", lineHeight: "36px", fontWeight: 700 },
  sectionTitle: { fontSize: "18px", lineHeight: "27px", fontWeight: 600 },
  body: { fontSize: "14px", lineHeight: "21px", fontWeight: 400 },
  caption: { fontSize: "12px", lineHeight: "18px", fontWeight: 400 },
} as const;

export const breakpoint = {
  narrowMin: "360px",
  compactMin: "768px",
  wideMin: "1280px",
} as const;

export const layout = {
  desktopCanvas: { width: "1440px", height: "960px" },
  navigationWidth: "216px",
  contentPadding: "32px",
  sectionGap: "24px",
} as const;

export const icon = {
  library: "lucide-react",
  strokeWidth: 2,
  sizes: { sm: "16px", md: "20px", lg: "24px" },
} as const;

export const theme = { color, spacing, radius, typography, breakpoint, layout, icon } as const;

export type Theme = typeof theme;
export type StatusTone = keyof typeof color.status;

/** Status must include text and/or shape/border; color alone is never sufficient. */
export const statusUsageGuidance =
  "Pair every status color with a visible text label and, where useful, icon or border. Never communicate status by color alone.";

/** Use Lucide React icons consistently; do not use icon-only controls without an accessible name. */
export const iconUsageGuidance =
  "Use Lucide React outline icons at the shared size and stroke width. Decorative icons are aria-hidden; icon-only controls require an accessible name.";
