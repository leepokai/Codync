package com.codync.android

/** The channel carries UTF-8 text, so unsupported BodyInit values must never turn into an empty write. */
internal fun miniAppFetchBodyScript(): String = """
  function miniAppFetchBody(request, init) {
    const body = init && init.body;
    if (body != null) {
      if (typeof body !== "string") {
        throw new TypeError("Codync mini-app fetch supports string bodies only. FormData, Blob, URLSearchParams and binary/stream bodies are not supported.");
      }
      return body;
    }
    // A null or undefined init.body inherits the input Request's body under Fetch rules.
    if (request && request.body !== null) {
      throw new TypeError("Codync mini-app fetch cannot relay a Request body. Pass its text explicitly as fetch(url, { method, body: text }).");
    }
    return "";
  }
"""
