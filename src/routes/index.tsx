import { createFileRoute } from "@tanstack/react-router";
import { Section } from "../components/Section";
import { AnimatedGradientDashBorder } from "../components/OwlMark";
import { Metrics } from "../components/Metrics";
import { config } from "../lib/config";

export const Route = createFileRoute("/")({
  component: HomePage,
});

function HomePage() {
  return (
    <main className="max-w-[80ch] mx-auto px-4 py-8 md:py-12">
      {/* Header with animated border */}
      <header className="mb-8 text-center">
        <AnimatedGradientDashBorder title={`${config.name} 🦉`} />
        <div className="text-[var(--ink-light)] mt-2">{config.tagline}</div>
      </header>

      {/* Addresses - prominent at top */}
      <div className="mb-6 border border-dashed border-[var(--rule)] p-4">
        <div>
          <strong>VOTE</strong> {"     "} {config.voteAccount}
        </div>
        <div>
          <strong>IDENTITY</strong> {"  "} {config.identity}
        </div>
        <div>
          <strong>NETWORK</strong> {"   "} mainnet-beta
        </div>
      </div>

      {/* About */}
      <Section id="about" title="About">
        <p>
          Operated by <strong>Gabe Rodriguez</strong> (
          <a href={`https://x.com/${config.contact.twitter}`} target="_blank" rel="noopener noreferrer">
            @{config.contact.twitter}
          </a>
          ), a core contributor to Solana&apos;s Agave validator client and on-chain programs at Anza. A way to
          experience Solana from the operator&apos;s seat, not just the codebase.
        </p>
      </Section>

      {/* Metrics */}
      <Section id="metrics" title="Metrics">
        <Metrics />
      </Section>

      {/* Links */}
      <Section id="links" title="Links">
        <div className="space-y-1">
          <div>
            <a href={config.links.stakewiz} target="_blank" rel="noopener noreferrer">
              stakewiz ↗
            </a>
          </div>
          <div>
            <a href={config.links.solscan} target="_blank" rel="noopener noreferrer">
              solscan ↗
            </a>
          </div>
          <div>
            <a href={config.links.validatorsApp} target="_blank" rel="noopener noreferrer">
              validators.app ↗
            </a>
          </div>
          <div>
            <a href={config.links.sfdp} target="_blank" rel="noopener noreferrer">
              solana foundation delegation program ↗
            </a>
          </div>
          <div>
            <a href={config.links.jito} target="_blank" rel="noopener noreferrer">
              jito stakenet ↗
            </a>
          </div>
          <div>
            <a href={config.links.ibrl} target="_blank" rel="noopener noreferrer">
              ibrl ↗
            </a>
          </div>
        </div>
      </Section>

      {/* Changelog */}
      <Section id="changelog" title="Changelog">
        <div>
          {config.changelog.map((entry, i) => (
            <div key={i}>
              <strong>{entry.date}</strong> {"  "} {entry.event}
            </div>
          ))}
        </div>
      </Section>
    </main>
  );
}
