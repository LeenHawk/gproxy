import { ConnectPanel } from "@/components/public/connect-panel"
import { FunnelClaims } from "@/components/public/funnel-claims"
import { PerformanceStats } from "@/components/public/performance-stats"
import { PublicFooter } from "@/components/public/public-footer"
import { PublicHeader } from "@/components/public/public-header"
import { PublicHero } from "@/components/public/public-hero"

export function PublicSite() {
  return (
    <div className="public-site">
      <PublicHeader />
      <main className="public-main">
        <PublicHero />
        <FunnelClaims />
        <PerformanceStats />
        <ConnectPanel />
      </main>
      <PublicFooter />
    </div>
  )
}
