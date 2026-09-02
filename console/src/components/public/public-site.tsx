import { ConnectPanel } from "@/components/public/connect-panel"
import { FunnelClaims } from "@/components/public/funnel-claims"
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
        <ConnectPanel />
      </main>
      <PublicFooter />
    </div>
  )
}
