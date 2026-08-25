import { ClaimRail } from "@/components/public/claim-rail"
import { PublicHeader } from "@/components/public/public-header"
import { PublicHero } from "@/components/public/public-hero"

export function PublicSite() {
  return (
    <div className="public-site">
      <PublicHeader />
      <main className="public-main">
        <PublicHero />
        <ClaimRail />
      </main>
    </div>
  )
}
