/* eslint-disable */
// ─────────────────────────────────────────────────────────────
// app.jsx — Atelier v2 (light + dark, theme-switchable tokens)
// Tweaks panel switches accent (Persian / Ink / Bordeaux) globally.
// Console kept below as reference (no theming).
// ─────────────────────────────────────────────────────────────

const ARTBOARD_W = 1280;
const ARTBOARD_H = 820;

const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "accent": "#7E1F2A"
}/*EDITMODE-END*/;

const ACCENT_HEX = ['#1E3A8A', '#1A1B20', '#7E1F2A'];
const ACCENT_KEYS = ['persian', 'ink', 'bordeaux'];
const ACCENT_LABELS = ['Persian', 'Ink', 'Bordeaux'];

function App() {
  const [t, setTweak] = useTweaks(TWEAK_DEFAULTS);
  const idx = ACCENT_HEX.indexOf((t.accent || '').toUpperCase());
  const accent = ACCENT_KEYS[idx >= 0 ? idx : 0];
  const accentLabel = ACCENT_LABELS[idx >= 0 ? idx : 0];

  return (
    <window.AtelierContext.Provider value={{ accent }}>
      <DesignCanvas
        storageKey="wotold-redesign-v2"
        title="Wotold · Redesign · Atelier v2"
        subtitle="New palette — cooler neutrals, deep cobalt accent (default), red only as signal. Toggle accent + compare light vs dark side-by-side. Try Tweaks to swap accent."
      >
        {/* ─────────────────────────────────────────
            Atelier · LIGHT
            ───────────────────────────────────────── */}
        <DCSection
          id="atelier-light"
          title="Atelier · Light"
          subtitle="Source Serif 4 + DM Sans + JetBrains Mono. Cool neutral paper, accent reserved, red is signal-only."
        >
          <DCArtboard id="l1" label="01 · Onboarding" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteOnboarding theme="light" />
          </DCArtboard>
          <DCArtboard id="l2" label="02 · Home" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteHome theme="light" />
          </DCArtboard>
          <DCArtboard id="l3" label="03 · Recording" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteRecording theme="light" />
          </DCArtboard>
          <DCArtboard id="l4" label="04 · Calls list" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteCallsList theme="light" />
          </DCArtboard>
          <DCArtboard id="l5" label="05 · Call · Transcript" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteCallDetailTranscript theme="light" />
          </DCArtboard>
          <DCArtboard id="l6" label="06 · Call · Recap" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteCallDetailRecap theme="light" />
          </DCArtboard>
          <DCArtboard id="l7" label="07 · Speaker confirm" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteSpeakerConfirm theme="light" />
          </DCArtboard>
          <DCArtboard id="l8" label="08 · Contacts" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteContacts theme="light" />
          </DCArtboard>
          <DCArtboard id="l9" label="09 · Settings · BYO keys" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteSettings theme="light" />
          </DCArtboard>
        </DCSection>

        {/* ─────────────────────────────────────────
            Atelier · DARK
            ───────────────────────────────────────── */}
        <DCSection
          id="atelier-dark"
          title="Atelier · Dark"
          subtitle="Same tokens, flipped. Subtle warm cast on ink so type still feels papery at night."
        >
          <DCArtboard id="d1" label="01 · Onboarding" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteOnboarding theme="dark" />
          </DCArtboard>
          <DCArtboard id="d2" label="02 · Home" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteHome theme="dark" />
          </DCArtboard>
          <DCArtboard id="d3" label="03 · Recording" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteRecording theme="dark" />
          </DCArtboard>
          <DCArtboard id="d4" label="04 · Calls list" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteCallsList theme="dark" />
          </DCArtboard>
          <DCArtboard id="d5" label="05 · Call · Transcript" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteCallDetailTranscript theme="dark" />
          </DCArtboard>
          <DCArtboard id="d6" label="06 · Call · Recap" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteCallDetailRecap theme="dark" />
          </DCArtboard>
          <DCArtboard id="d7" label="07 · Speaker confirm" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteSpeakerConfirm theme="dark" />
          </DCArtboard>
          <DCArtboard id="d8" label="08 · Contacts" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteContacts theme="dark" />
          </DCArtboard>
          <DCArtboard id="d9" label="09 · Settings · BYO keys" width={ARTBOARD_W} height={ARTBOARD_H}>
            <AteSettings theme="dark" />
          </DCArtboard>
        </DCSection>

        {/* ─────────────────────────────────────────
            B · Console (kept for reference)
            ───────────────────────────────────────── */}
        <DCSection
          id="console"
          title="B · Console (reference)"
          subtitle="Pro-tool direction from round one. Kept available in case you want to revisit."
        >
          <DCArtboard id="b1" label="01 · Onboarding" width={ARTBOARD_W} height={ARTBOARD_H}>
            <ConOnboarding />
          </DCArtboard>
          <DCArtboard id="b2" label="02 · Studio" width={ARTBOARD_W} height={ARTBOARD_H}>
            <ConHome />
          </DCArtboard>
          <DCArtboard id="b3" label="03 · Recording" width={ARTBOARD_W} height={ARTBOARD_H}>
            <ConRecording />
          </DCArtboard>
          <DCArtboard id="b5" label="05 · Call · Transcript" width={ARTBOARD_W} height={ARTBOARD_H}>
            <ConCallDetailTranscript />
          </DCArtboard>
          <DCArtboard id="b7" label="07 · Identify speaker" width={ARTBOARD_W} height={ARTBOARD_H}>
            <ConSpeakerConfirm />
          </DCArtboard>
        </DCSection>
      </DesignCanvas>

      {/* ─── Tweaks panel ─── */}
      <TweaksPanel title="Wotold Tweaks">
        <TweakSection label="Accent color" />
        <TweakColor
          label={accentLabel}
          value={ACCENT_HEX[idx >= 0 ? idx : 0]}
          options={ACCENT_HEX}
          onChange={(v) => setTweak('accent', v)}
        />
        <div style={{
          marginTop: 4, marginBottom: 12,
          padding: '0 4px',
          fontSize: 11, lineHeight: 1.5,
          color: '#888',
          fontFamily: 'ui-monospace, JetBrains Mono, monospace',
        }}>
          Applies to every Atelier artboard (light + dark).
          Red stays reserved for record / danger signal.
        </div>
      </TweaksPanel>
    </window.AtelierContext.Provider>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<App />);
