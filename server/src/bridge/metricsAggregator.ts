import type { IndexedEvent, ProtocolMetrics } from "../types.js";

/**
 * Cumulative ProtocolMetrics built purely from the event categories the indexer
 * currently decodes (see tools/indexer/src/indexer.rs::decode_event — only
 * vouch/loan create|increase|decrease|withdraw|request|repay|slash carry structured
 * fields today). Mirrors the aggregation approach in
 * tools/indexer/src/metrics.rs::IndexerMetrics::record_event, ported to TS so this
 * service doesn't have to shell out to the Rust binary. Fields the indexer doesn't
 * yet decode (protocol fee revenue) are reported as 0 rather than fabricated.
 */
export class MetricsAggregator {
  private totalLoans = 0;
  private activeLoans = 0;
  private defaultedLoans = 0;
  private totalYieldDistributed = 0;
  private activeLoanPrincipal = 0;
  private readonly borrowerVolume = new Map<string, number>();
  private readonly voucherStake = new Map<string, number>();

  applyEvent(event: IndexedEvent): ProtocolMetrics {
    const v = event.value;
    const num = (key: string): number => {
      const raw = v[key];
      return typeof raw === "number" ? raw : typeof raw === "string" ? Number(raw) || 0 : 0;
    };
    const str = (key: string): string | undefined =>
      typeof v[key] === "string" ? (v[key] as string) : undefined;

    switch (`${event.category}/${event.action}`) {
      case "loan/request": {
        this.totalLoans += 1;
        this.activeLoans += 1;
        const amount = num("amount_stroops");
        this.activeLoanPrincipal += amount;
        const borrower = str("borrower");
        if (borrower) this.borrowerVolume.set(borrower, (this.borrowerVolume.get(borrower) ?? 0) + amount);
        break;
      }
      case "loan/repay": {
        this.activeLoans = Math.max(0, this.activeLoans - 1);
        this.activeLoanPrincipal = Math.max(0, this.activeLoanPrincipal - num("payment_stroops"));
        this.totalYieldDistributed += num("payment_stroops");
        break;
      }
      case "loan/slash": {
        this.activeLoans = Math.max(0, this.activeLoans - 1);
        this.defaultedLoans += 1;
        this.activeLoanPrincipal = Math.max(0, this.activeLoanPrincipal - num("total_slashed_stroops"));
        break;
      }
      case "vouch/create":
      case "vouch/increase": {
        const voucher = str("voucher");
        const stake = num("stake_stroops");
        if (voucher) this.voucherStake.set(voucher, (this.voucherStake.get(voucher) ?? 0) + stake);
        break;
      }
      case "vouch/decrease": {
        const voucher = str("voucher");
        if (voucher) {
          const current = this.voucherStake.get(voucher) ?? 0;
          this.voucherStake.set(voucher, Math.max(0, current - num("stake_stroops")));
        }
        break;
      }
      case "vouch/withdraw": {
        const voucher = str("voucher");
        if (voucher) this.voucherStake.delete(voucher);
        break;
      }
      default:
        break;
    }

    return this.snapshot(event.id);
  }

  private snapshot(eventTimestamp: number): ProtocolMetrics {
    const totalStake = sum(this.voucherStake.values());
    return {
      tvl: this.activeLoanPrincipal + totalStake,
      active_loans: this.activeLoans,
      total_loans: this.totalLoans,
      defaulted_loans: this.defaultedLoans,
      default_rate: this.totalLoans === 0 ? 0 : this.defaultedLoans / this.totalLoans,
      total_yield_distributed: this.totalYieldDistributed,
      slash_count: this.defaultedLoans,
      fee_revenue: 0,
      top_borrowers: topN(this.borrowerVolume, 5),
      top_vouchers: topN(this.voucherStake, 5),
      timestamp: eventTimestamp,
    };
  }
}

function sum(values: Iterable<number>): number {
  let total = 0;
  for (const v of values) total += v;
  return total;
}

function topN(map: Map<string, number>, n: number): [string, number][] {
  return [...map.entries()].sort((a, b) => b[1] - a[1]).slice(0, n);
}
