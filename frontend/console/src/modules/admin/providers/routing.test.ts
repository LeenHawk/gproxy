import { describe, expect, it } from "vitest";

import {
  buildRoutingDocument,
  routingDraftsFromDocument,
  normalizeRoutingDrafts,
} from "./routing";

describe("providers routing helpers", () => {
  it("round-trips drafts through canonical routing document", () => {
    const drafts = [
      {
        id: "rule-1",
        srcOperation: "generate_content",
        srcProtocol: "openai",
        implementation: "Passthrough" as const,
        destinationOperation: "",
        destinationProtocol: "",
      },
      {
        id: "rule-2",
        srcOperation: "count_tokens",
        srcProtocol: "claude",
        implementation: "TransformTo" as const,
        destinationOperation: "count_tokens",
        destinationProtocol: "openai",
      },
      {
        id: "rule-3",
        srcOperation: "gemini_live",
        srcProtocol: "gemini",
        implementation: "Local" as const,
        destinationOperation: "",
        destinationProtocol: "",
      },
      {
        id: "rule-4",
        srcOperation: "openai_response_websocket",
        srcProtocol: "openai",
        implementation: "Unsupported" as const,
        destinationOperation: "",
        destinationProtocol: "",
      },
    ];

    const document = buildRoutingDocument(drafts);
    const roundtrip = routingDraftsFromDocument(document);

    expect(roundtrip).toHaveLength(4);
    expect(roundtrip.map((rule) => rule.implementation)).toEqual([
      "Passthrough",
      "TransformTo",
      "Local",
      "Unsupported",
    ]);
    expect(roundtrip[1].destinationOperation).toBe("count_tokens");
    expect(roundtrip[1].destinationProtocol).toBe("openai");
  });

  it("rejects duplicate source routes", () => {
    expect(() =>
      normalizeRoutingDrafts([
        {
          id: "rule-1",
          srcOperation: "generate_content",
          srcProtocol: "openai",
          implementation: "Passthrough",
          destinationOperation: "",
          destinationProtocol: "",
        },
        {
          id: "rule-2",
          srcOperation: "generate_content",
          srcProtocol: "openai",
          implementation: "Unsupported",
          destinationOperation: "",
          destinationProtocol: "",
        },
      ]),
    ).toThrow(/duplicate/i);
  });
});
