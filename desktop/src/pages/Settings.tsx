// Settings.tsx — v21.1: Settings & Preferences page
import { useState } from "react";
import { colors, fonts } from "../theme";
import { t } from "../i18n";
import { type AppSettings, type Currency, type Language, type ThemeMode, DEFAULT_SETTINGS } from "../hooks/useSettings";

interface SettingsProps {
  settings: AppSettings;
  onUpdate: (patch: Partial<AppSettings>) => void;
  onReset:  () => void;
}

// ── Section wrapper ───────────────────────────────────────────────────────────

function Section({ title, icon, children }: { title: string; icon: string; children: React.ReactNode }) {
  return (
    <div style={{
      background: colors.surface, border: `1px solid ${colors.border}`,
      borderRadius: 14, overflow: "hidden", marginBottom: 16,
    }}>
      <div style={{
        padding: "14px 20px", borderBottom: `1px solid ${colors.border}`,
        display: "flex", alignItems: "center", gap: 10,
      }}>
        <span style={{ fontSize: 16 }}>{icon}</span>
        <span style={{ fontWeight: 700, fontSize: 14, color: colors.text }}>{title}</span>
      </div>
      <div style={{ padding: "4px 0" }}>{children}</div>
    </div>
  );
}

// ── Setting row ───────────────────────────────────────────────────────────────

function Row({ label, description, children }: {
  label: string; description?: string; children: React.ReactNode;
}) {
  return (
    <div style={{
      display: "flex", alignItems: "center", justifyContent: "space-between",
      padding: "14px 20px", borderBottom: `1px solid ${colors.border}`,
      gap: 20,
    }}>
      <div>
        <div style={{ fontSize: 14, color: colors.text, fontWeight: 500 }}>{label}</div>
        {description && (
          <div style={{ fontSize: 12, color: colors.muted, marginTop: 2 }}>{description}</div>
        )}
      </div>
      <div style={{ flexShrink: 0 }}>{children}</div>
    </div>
  );
}

// ── Segmented control ─────────────────────────────────────────────────────────

function Segmented<T extends string>({
  value, options, onChange,
}: {
  value: T;
  options: { id: T; label: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div style={{
      display: "flex", background: colors.surface2,
      border: `1px solid ${colors.border}`, borderRadius: 8, padding: 3, gap: 3,
    }}>
      {options.map(o => (
        <button key={o.id} onClick={() => onChange(o.id)} style={{
          padding: "5px 16px", border: "none", borderRadius: 6, cursor: "pointer",
          fontFamily: fonts.sans, fontWeight: 600, fontSize: 13,
          background: value === o.id ? colors.accent : "transparent",
          color: value === o.id ? "#000" : colors.muted,
          transition: "all .2s",
        }}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

// ── URL input with test button ────────────────────────────────────────────────

function UrlInput({
  value, onChange, placeholder,
}: { value: string; onChange: (v: string) => void; placeholder: string }) {
  const [status, setStatus] = useState<"idle" | "testing" | "ok" | "err">("idle");

  async function test() {
    setStatus("testing");
    const ctrl = new AbortController();
    const timer = setTimeout(() => ctrl.abort(), 5000);
    try {
      const res = await fetch(value.replace(/\/+$/, "") + "/api/testnet/summary", { signal: ctrl.signal });
      clearTimeout(timer);
      setStatus(res.ok ? "ok" : "err");
    } catch {
      clearTimeout(timer);
      setStatus("err");
    }
    setTimeout(() => setStatus("idle"), 3000);
  }

  const statusColor = status === "ok" ? colors.green : status === "err" ? colors.red : colors.muted;
  const statusLabel = status === "testing" ? t.testing : status === "ok" ? t.test_ok : status === "err" ? t.test_err : t.test;

  return (
    <div style={{ display: "flex", gap: 8, alignItems: "center", width: 380 }}>
      <input
        value={value}
        onChange={e => onChange(e.target.value)}
        placeholder={placeholder}
        style={{
          flex: 1, background: colors.surface2,
          border: `1px solid ${colors.border}`, borderRadius: 8,
          padding: "8px 12px", color: colors.text,
          fontFamily: fonts.mono, fontSize: 12, outline: "none",
        }}
        onFocus={e => (e.currentTarget.style.borderColor = colors.accent)}
        onBlur={e  => (e.currentTarget.style.borderColor = colors.border)}
      />
      <button onClick={test} disabled={status === "testing"} style={{
        padding: "8px 14px", borderRadius: 8,
        background: status === "idle" ? colors.surface2 : statusColor + "22",
        border: `1px solid ${status === "idle" ? colors.border : statusColor}`,
        color: statusColor, cursor: status === "testing" ? "wait" : "pointer",
        fontFamily: fonts.sans, fontSize: 12, fontWeight: 600,
        transition: "all .2s", whiteSpace: "nowrap",
      }}>
        {statusLabel}
      </button>
    </div>
  );
}

// ── Poll interval slider ──────────────────────────────────────────────────────

function PollSlider({ value, onChange }: { value: number; onChange: (v: number) => void }) {
  const OPTIONS = [4, 8, 15, 30, 60];
  return (
    <div style={{ display: "flex", gap: 6 }}>
      {OPTIONS.map(v => (
        <button key={v} onClick={() => onChange(v)} style={{
          padding: "5px 12px", borderRadius: 6, border: "none", cursor: "pointer",
          fontFamily: fonts.mono, fontSize: 12, fontWeight: 600,
          background: value === v ? `${colors.blue}22` : colors.surface2,
          color: value === v ? colors.blue : colors.muted,
          outline: value === v ? `1px solid ${colors.blue}55` : "none",
          transition: "all .2s",
        }}>
          {v}s
        </button>
      ))}
    </div>
  );
}

// ── Danger zone ───────────────────────────────────────────────────────────────

function DangerZone({ onReset }: { onReset: () => void }) {
  const [confirm, setConfirm] = useState(false);
  return (
    <div style={{ padding: "14px 20px", display: "flex", alignItems: "center", justifyContent: "space-between" }}>
      <div>
        <div style={{ fontSize: 14, color: colors.text, fontWeight: 500 }}>{t.reset_label}</div>
        <div style={{ fontSize: 12, color: colors.muted, marginTop: 2 }}>{t.reset_desc}</div>
      </div>
      {!confirm ? (
        <button onClick={() => setConfirm(true)} style={{
          padding: "7px 18px", border: `1px solid ${colors.red}55`,
          borderRadius: 8, background: `${colors.red}10`,
          color: colors.red, cursor: "pointer",
          fontFamily: fonts.sans, fontSize: 13, fontWeight: 600,
        }}>{t.reset}</button>
      ) : (
        <div style={{ display: "flex", gap: 8 }}>
          <button onClick={() => setConfirm(false)} style={{
            padding: "7px 14px", border: `1px solid ${colors.border}`,
            borderRadius: 8, background: colors.surface2,
            color: colors.muted, cursor: "pointer",
            fontFamily: fonts.sans, fontSize: 13,
          }}>{t.cancel}</button>
          <button onClick={() => { onReset(); setConfirm(false); }} style={{
            padding: "7px 18px", border: "none",
            borderRadius: 8, background: colors.red,
            color: "#fff", cursor: "pointer",
            fontFamily: fonts.sans, fontSize: 13, fontWeight: 700,
          }}>{t.confirm_reset}</button>
        </div>
      )}
    </div>
  );
}

// ── Main ─────────────────────────────────────────────────────────────────────

export function Settings({ settings, onUpdate, onReset }: SettingsProps) {
  const [saved, setSaved] = useState(false);

  function handleUpdate(patch: Partial<AppSettings>) {
    onUpdate(patch);
    setSaved(true);
    setTimeout(() => setSaved(false), 1500);
  }

  return (
    <div style={{ maxWidth: 720, margin: "0 auto" }}>
      {/* Header */}
      <div style={{
        display: "flex", justifyContent: "space-between", alignItems: "center",
        marginBottom: 24,
      }}>
        <div>
          <h2 style={{ margin: 0, fontSize: 22, fontWeight: 800, color: colors.text }}>
            {t.settings_title}
          </h2>
          <p style={{ margin: "4px 0 0", fontSize: 13, color: colors.muted }}>
            {t.settings_subtitle}
          </p>
        </div>
        {saved && (
          <span style={{
            fontSize: 12, color: colors.green, fontWeight: 700,
            padding: "5px 12px", borderRadius: 6,
            background: `${colors.green}18`, border: `1px solid ${colors.green}44`,
          }}>{t.saved}</span>
        )}
      </div>

      {/* Network */}
      <Section title={t.section_network} icon="🌐">
        <Row label={t.row_testnet_url} description="REST API endpoint for testnet">
          <UrlInput
            value={settings.nodeUrlTestnet}
            onChange={v => handleUpdate({ nodeUrlTestnet: v })}
            placeholder={DEFAULT_SETTINGS.nodeUrlTestnet}
          />
        </Row>
        <Row label={t.row_mainnet_url} description="REST API endpoint for mainnet">
          <UrlInput
            value={settings.nodeUrlMainnet}
            onChange={v => handleUpdate({ nodeUrlMainnet: v })}
            placeholder={DEFAULT_SETTINGS.nodeUrlMainnet}
          />
        </Row>
        <Row label={t.row_poll} description="Live dashboard refresh rate">
          <PollSlider value={settings.pollInterval} onChange={v => handleUpdate({ pollInterval: v })} />
        </Row>
      </Section>

      {/* Appearance */}
      <Section title={t.section_appearance} icon="🎨">
        <Row label={t.row_theme} description={t.row_theme_desc}>
          <Segmented<ThemeMode>
            value={settings.theme}
            options={[
              { id: "auto",  label: t.theme_auto  },
              { id: "dark",  label: t.theme_dark  },
              { id: "light", label: t.theme_light },
            ]}
            onChange={v => handleUpdate({ theme: v })}
          />
        </Row>
        <Row label={t.row_currency} description="Display PKT amounts in">
          <Segmented<Currency>
            value={settings.currency}
            options={[{ id: "PKT", label: "PKT" }, { id: "USD", label: "USD" }]}
            onChange={v => handleUpdate({ currency: v })}
          />
        </Row>
      </Section>

      {/* Language */}
      <Section title={t.section_language} icon="🌏">
        <Row label={t.row_language}>
          <Segmented<Language>
            value={settings.language}
            options={[{ id: "en", label: t.lang_en }, { id: "vi", label: t.lang_vi }]}
            onChange={v => handleUpdate({ language: v })}
          />
        </Row>
      </Section>

      {/* About */}
      <Section title={t.section_about} icon="ℹ">
        <Row label={t.row_version} description="PKTScan Desktop">
          <span style={{ fontFamily: fonts.mono, fontSize: 13, color: colors.muted }}>v21.0</span>
        </Row>
        <Row label={t.row_stack} description="Tauri v2 + React 18 + TypeScript">
          <div style={{ display: "flex", gap: 6 }}>
            {["Rust", "Tauri 2", "React 18", "Vite"].map(tech => (
              <span key={tech} style={{
                fontSize: 10, padding: "2px 7px", borderRadius: 4,
                background: colors.surface2, color: colors.muted,
                border: `1px solid ${colors.border}`, fontFamily: fonts.mono,
              }}>{tech}</span>
            ))}
          </div>
        </Row>
        <Row label={t.row_source} description="PKT blockchain explorer">
          <span style={{ fontFamily: fonts.mono, fontSize: 12, color: colors.blue }}>
            oceif.com/blockchain-rust
          </span>
        </Row>
      </Section>

      {/* Danger zone */}
      <Section title={t.section_danger} icon="⚠">
        <DangerZone onReset={onReset} />
      </Section>
    </div>
  );
}
