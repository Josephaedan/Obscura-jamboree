// Reads Carousell search result cards out of the live DOM.
// Returns a JSON string: { blocked, title, url, listings: [...] }.
(function () {
  var PRICE_RE = /^(S\$|RM|HK\$|NT\$|PHP|₱|Rp|\$)\s?[\d,]+(\.\d+)?$/;
  var FREE_RE = /^free$/i;
  var TIME_RE = /(\bago$|^yesterday$|^today$|^just now$)/i;
  var CONDITION_RE = /^(brand new|like new|lightly used|well used|heavily used|new|used)$/i;
  var LISTING_ID_RE = /\/p\/(?:[^/?#]*-)?(\d+)\/?(?:[?#]|$)/;

  var title = document.title || "";
  var bodyText = document.body ? document.body.textContent || "" : "";
  var blocked =
    /just a moment/i.test(title) ||
    /attention required/i.test(title) ||
    !!document.querySelector("#challenge-form, #cf-challenge-running, .cf-turnstile") ||
    /verify you are human/i.test(bodyText.slice(0, 4000));

  function abs(href) {
    try {
      return new URL(href, location.href).href;
    } catch (e) {
      return href || null;
    }
  }

  function clean(s) {
    return (s || "").replace(/\s+/g, " ").trim();
  }

  // Leaf-ish text chunks of an element, in document order, de-duplicated.
  function texts(root) {
    var out = [];
    var seen = {};
    var nodes = root.querySelectorAll("p, span, h1, h2, h3, h4, div");
    for (var i = 0; i < nodes.length; i++) {
      var n = nodes[i];
      if (n.querySelector("p, span, h1, h2, h3, h4, div")) continue;
      var t = clean(n.textContent);
      if (!t || seen[t]) continue;
      seen[t] = true;
      out.push({ text: t, node: n });
    }
    return out;
  }

  function productLink(card) {
    var links = card.querySelectorAll('a[href*="/p/"]');
    return links.length ? links[0] : null;
  }

  function parseCard(card) {
    var link = productLink(card);
    if (!link) return null;
    var href = abs(link.getAttribute("href"));
    var url = href ? href.split("?")[0].split("#")[0] : null;

    var id = null;
    var testid = card.getAttribute("data-testid") || "";
    var m = testid.match(/^listing-card-(\d+)/);
    if (m) id = m[1];
    if (!id && href) {
      var m2 = href.match(LISTING_ID_RE);
      if (m2) id = m2[1];
    }

    var sellerLink = card.querySelector('a[href*="/u/"]');
    var seller = null;
    if (sellerLink) {
      var sm = (sellerLink.getAttribute("href") || "").match(/\/u\/([^/?#]+)/);
      seller = sm ? decodeURIComponent(sm[1]) : clean(sellerLink.textContent) || null;
    }

    var chunks = texts(card);
    var price = null;
    var originalPrice = null;
    var posted = null;
    var condition = null;
    var name = null;

    for (var i = 0; i < chunks.length; i++) {
      var t = chunks[i].text;
      if (PRICE_RE.test(t) || FREE_RE.test(t)) {
        var struck = chunks[i].node.closest && chunks[i].node.closest("s, del, strike");
        if (struck) {
          if (!originalPrice) originalPrice = t;
        } else if (!price) {
          price = t;
        } else if (!originalPrice) {
          originalPrice = t;
        }
        continue;
      }
      if (!posted && TIME_RE.test(t)) {
        posted = t;
        continue;
      }
      if (!condition && CONDITION_RE.test(t)) {
        condition = t;
        continue;
      }
    }

    // Title: the product image alt or the longest non-metadata text inside the
    // product link (the seller name lives outside it).
    var linkChunks = texts(link);
    for (var j = 0; j < linkChunks.length; j++) {
      var lt = linkChunks[j].text;
      if (PRICE_RE.test(lt) || FREE_RE.test(lt) || TIME_RE.test(lt) || CONDITION_RE.test(lt)) continue;
      if (seller && lt === seller) continue;
      if (!name || lt.length > name.length) name = lt;
    }
    var img = link.querySelector("img");
    if (!name && img) name = clean(img.getAttribute("alt")) || null;

    var image = null;
    if (img) {
      image = img.getAttribute("src") || img.getAttribute("data-src") || null;
      if (image && image.indexOf("data:") === 0) image = img.getAttribute("data-src") || null;
      if (image) image = abs(image);
    }

    return {
      id: id,
      title: name,
      price: price,
      original_price: originalPrice,
      condition: condition,
      seller: seller,
      posted: posted,
      url: url,
      image: image,
    };
  }

  var cards = Array.prototype.slice.call(document.querySelectorAll('[data-testid^="listing-card-"]'));
  // Only the outermost card element per listing.
  cards = cards.filter(function (c) {
    var p = c.parentElement;
    return !(p && p.closest && p.closest('[data-testid^="listing-card-"]'));
  });

  // Fallback when the test ids are absent: treat the nearest container that
  // holds exactly one product link as the card.
  if (!cards.length) {
    var anchors = document.querySelectorAll('a[href*="/p/"]');
    var seenCard = [];
    for (var a = 0; a < anchors.length; a++) {
      var el = anchors[a];
      var card = el;
      while (card.parentElement && card.parentElement !== document.body) {
        var parent = card.parentElement;
        var ids = {};
        var pl = parent.querySelectorAll('a[href*="/p/"]');
        for (var k = 0; k < pl.length; k++) {
          var mm = (pl[k].getAttribute("href") || "").match(LISTING_ID_RE);
          ids[mm ? mm[1] : pl[k].getAttribute("href")] = true;
        }
        if (Object.keys(ids).length > 1) break;
        card = parent;
      }
      if (seenCard.indexOf(card) === -1) seenCard.push(card);
    }
    cards = seenCard;
  }

  var listings = [];
  var seenIds = {};
  for (var c = 0; c < cards.length; c++) {
    var item = parseCard(cards[c]);
    if (!item) continue;
    var key = item.id || item.url;
    if (seenIds[key]) continue;
    seenIds[key] = true;
    listings.push(item);
  }

  return JSON.stringify({
    blocked: blocked && listings.length === 0,
    title: title,
    url: location.href,
    listings: listings,
  });
})()
