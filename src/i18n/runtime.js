let translateImpl = (key) => key;

export function setTranslationResolver(nextResolver) {
  translateImpl = nextResolver;
}

export function tr(key, params) {
  return translateImpl(key, params);
}
