'use strict';

class RateLimiter {
  /**
   * @param {object} opts
   * @param {number} opts.windowMs   - Sliding window duration in ms (default: 60_000)
   * @param {number} opts.maxRequests - Max requests per window (default: 100)
   */
  constructor({ windowMs = 60_000, maxRequests = 100 } = {}) {
    this.windowMs = windowMs;
    this.maxRequests = maxRequests;
    // Map<key, number[]> — stores timestamps of requests within the window
    this._store = new Map();
  }

  /** Returns true if the key is within the rate limit, false if exceeded. */
  check(key, chainId) {
    const now = Date.now();
    const cutoff = now - this.windowMs;
    const bucketKey = chainId ? `${key}:${chainId}` : key;
    const timestamps = (this._store.get(bucketKey) || []).filter(t => t > cutoff);
    if (timestamps.length >= this.maxRequests) {
      this._store.set(bucketKey, timestamps);
      return false;
    }
    timestamps.push(now);
    this._store.set(bucketKey, timestamps);
    return true;
  }

  /**
   * Returns an Express-compatible middleware function.
   * Checks API key (x-api-key header) first, then falls back to IP.
   */
  middleware() {
    return (req, res, next) => {
      const key = req.headers['x-api-key'] || req.ip || req.socket.remoteAddress;
      const chainId = req.headers['x-chain-id'];
      if (!this.check(key, chainId)) {
        res.setHeader('Retry-After', Math.ceil(this.windowMs / 1000));
        return res.status(429).json({ error: 'Too Many Requests' });
      }
      next();
    };
  }
}

module.exports = { RateLimiter };
