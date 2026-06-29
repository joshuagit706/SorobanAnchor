import { renderHook, act } from "@testing-library/react";
import { useAnchorPlayground, SEP_PROTOCOLS } from "./useAnchorPlayground";

const MOCK_JSON = { assets: [{ code: "USDC" }] };

global.fetch = jest.fn() as jest.Mock;

beforeEach(() => {
  jest.clearAllMocks();
  (global.fetch as jest.Mock).mockResolvedValue({
    ok: true,
    status: 200,
    headers: { get: () => "application/json" },
    json: () => Promise.resolve(MOCK_JSON),
    text: () => Promise.resolve(""),
  });
  Object.defineProperty(global, "performance", {
    value: { now: jest.fn(() => 0) },
    writable: true,
  });
  Object.defineProperty(global, "crypto", {
    value: { randomUUID: () => "uuid-1" },
    writable: true,
  });
});

describe("useAnchorPlayground", () => {
  it("initialises with SEP-1 and its first endpoint", () => {
    const { result } = renderHook(() => useAnchorPlayground());
    expect(result.current.activeSEP.id).toBe("sep1");
    expect(result.current.activeEp.id).toBe("stellar-toml");
    expect(result.current.params).toEqual({});
    expect(result.current.history).toHaveLength(0);
  });

  it("selectSEP switches protocol and resets state", () => {
    const { result } = renderHook(() => useAnchorPlayground());
    act(() => {
      result.current.selectSEP(SEP_PROTOCOLS.find(s => s.id === "sep6")!);
    });
    expect(result.current.activeSEP.id).toBe("sep6");
    expect(result.current.activeEp.id).toBe("sep6-info");
    expect(result.current.params).toEqual({});
    expect(result.current.response).toBeNull();
  });

  it("selectEp switches endpoint and resets params", () => {
    const { result } = renderHook(() => useAnchorPlayground());
    const sep6 = SEP_PROTOCOLS.find(s => s.id === "sep6")!;
    act(() => { result.current.selectSEP(sep6); });
    act(() => { result.current.selectEp(sep6.endpoints[1]); }); // sep6-deposit
    expect(result.current.activeEp.id).toBe("sep6-deposit");
    expect(result.current.params).toEqual({});
  });

  it("setParam updates params", () => {
    const { result } = renderHook(() => useAnchorPlayground());
    act(() => { result.current.setParam("asset_code", "USDC"); });
    expect(result.current.params).toEqual({ asset_code: "USDC" });
  });

  it("buildUrl includes query params for GET endpoints", () => {
    const { result } = renderHook(() => useAnchorPlayground());
    const sep6 = SEP_PROTOCOLS.find(s => s.id === "sep6")!;
    act(() => {
      result.current.selectSEP(sep6);
      result.current.selectEp(sep6.endpoints[1]); // sep6-deposit
      result.current.setParam("asset_code", "USDC");
    });
    expect(result.current.buildUrl()).toContain("asset_code=USDC");
  });

  it("sendRequest calls fetch and populates response", async () => {
    const { result } = renderHook(() => useAnchorPlayground());
    await act(async () => { await result.current.sendRequest(); });
    expect(global.fetch).toHaveBeenCalledWith(
      expect.stringContaining("/.well-known/stellar.toml"),
      expect.objectContaining({ method: "GET" })
    );
    expect(result.current.response).not.toBeNull();
    expect(result.current.response?.status).toBe(200);
    expect(result.current.history).toHaveLength(1);
    expect(result.current.history[0].success).toBe(true);
  });

  it("sendRequest records failed request in history", async () => {
    (global.fetch as jest.Mock).mockRejectedValueOnce(new Error("Network failure"));
    const { result } = renderHook(() => useAnchorPlayground());
    await act(async () => { await result.current.sendRequest(); });
    expect(result.current.error).toBe("Network failure");
    expect(result.current.history).toHaveLength(1);
    expect(result.current.history[0].success).toBe(false);
  });

  it("sendRequest sends Authorization header when jwt is set", async () => {
    const { result } = renderHook(() => useAnchorPlayground());
    act(() => { result.current.setJwt("my.jwt.token"); });
    await act(async () => { await result.current.sendRequest(); });
    expect(global.fetch).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ headers: expect.objectContaining({ Authorization: "Bearer my.jwt.token" }) })
    );
  });
});

// ─── Issue #565: localStorage persistence tests ───────────────────────────────

describe("useAnchorPlayground – requestHistory persistence", () => {
  const STORAGE_KEY = "anchorkit_playground_history_v1";

  beforeEach(() => {
    jest.clearAllMocks();
    localStorage.clear();
    (global.fetch as jest.Mock).mockResolvedValue({
      ok: true,
      status: 200,
      headers: { get: () => "application/json" },
      json: () => Promise.resolve(MOCK_JSON),
      text: () => Promise.resolve(""),
    });
    Object.defineProperty(global, "performance", {
      value: { now: jest.fn(() => 0) },
      writable: true,
    });
    Object.defineProperty(global, "crypto", {
      value: { randomUUID: () => "uuid-persist" },
      writable: true,
    });
  });

  it("hydrates requestHistory from localStorage on mount", () => {
    const existing = [
      { id: "h1", timestamp: 1000, operation: "SEP-1 /.well-known/stellar.toml", requestBody: {}, responseStatus: 200, responseBody: {}, durationMs: 50 },
    ];
    localStorage.setItem(STORAGE_KEY, JSON.stringify(existing));

    const { result } = renderHook(() => useAnchorPlayground());
    expect(result.current.requestHistory).toHaveLength(1);
    expect(result.current.requestHistory[0].id).toBe("h1");
  });

  it("corrupted localStorage falls back to empty array", () => {
    localStorage.setItem(STORAGE_KEY, "not-valid-json{{{}");
    const { result } = renderHook(() => useAnchorPlayground());
    expect(result.current.requestHistory).toHaveLength(0);
  });

  it("new requests are prepended to requestHistory and persisted", async () => {
    const { result } = renderHook(() => useAnchorPlayground());
    await act(async () => { await result.current.sendRequest(); });

    expect(result.current.requestHistory).toHaveLength(1);
    expect(result.current.requestHistory[0].responseStatus).toBe(200);

    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY)!);
    expect(stored).toHaveLength(1);
    expect(stored[0].responseStatus).toBe(200);
  });

  it("requestHistory is truncated at MAX_HISTORY_ENTRIES (50)", async () => {
    // Pre-fill with 50 entries
    const existing = Array.from({ length: 50 }, (_, i) => ({
      id: `h${i}`, timestamp: i, operation: "op", requestBody: {}, responseStatus: 200, responseBody: {}, durationMs: 1,
    }));
    localStorage.setItem(STORAGE_KEY, JSON.stringify(existing));

    const { result } = renderHook(() => useAnchorPlayground());
    await act(async () => { await result.current.sendRequest(); });

    expect(result.current.requestHistory).toHaveLength(50);
  });

  it("clearHistory removes localStorage entry and resets state", async () => {
    const { result } = renderHook(() => useAnchorPlayground());
    await act(async () => { await result.current.sendRequest(); });
    expect(result.current.requestHistory).toHaveLength(1);

    act(() => { result.current.clearHistory(); });
    expect(result.current.requestHistory).toHaveLength(0);
    expect(localStorage.getItem(STORAGE_KEY)).toBeNull();
  });
});
