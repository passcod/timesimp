import test from "ava";

import { Timesimp } from "../index.js";

// SKIP: Test works, but ava hangs

test.skip("works", async (t) => {
  t.timeout(1000);

  const server = new Timesimp(
    async (err) => {
      if (err) throw err;
      return 0;
    },
    async (err) => {
      if (err) throw err;
    },
    async (err) => {
      if (err) throw err;
      throw new Error("Not implemented (server has no upstream)");
    },
  );

  let store = null;
  const client = new Timesimp(
    async (err) => {
      if (err) throw err;
      return store;
    },
    async (err, offset) => {
      if (err) throw err;
      store = offset;
    },
    async (err, request) => {
      if (err) throw err;
      return server.answerClient(request);
    },
  );

  const offset = await client.attemptSync({
    jitter: 1000,
  });
  t.assert(
    offset > -1000 && offset < 1000,
    `Offset should be within 1ms, is ${offset}us`,
  );
});
