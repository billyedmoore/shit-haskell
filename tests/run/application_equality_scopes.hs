{-# LANGUAGE AllowAmbiguousTypes, FlexibleContexts, FlexibleInstances, GADTs,
             MultiParamTypeClasses, NoPolyKinds, RankNTypes, ScopedTypeVariables,
             TypeApplications, TypeFamilies, UndecidableSuperClasses #-}
module Main where

data Parsed
data Proxy a = Proxy
type family Body b p where Body b Parsed = b
class (b ~ Body b p) => HasBody b p
instance HasBody b Parsed
class Tag a where tag :: Proxy a -> Int
instance Tag Int where tag _ = 17
instance Tag Bool where tag _ = 29

use :: (forall b p. (HasBody b p, Tag b) => Proxy b -> Proxy p -> Proxy (Body (Body b p) p) -> Int) -> Int
use f = f (Proxy :: Proxy Int) (Proxy :: Proxy Parsed) Proxy
      + f (Proxy :: Proxy Bool) (Proxy :: Proxy Parsed) Proxy

outer :: forall b p. (HasBody b p, Tag b) => Proxy (Body (Body b p) p) -> Int
outer x = use inner + tag x
  where
    inner :: forall c q. (HasBody c q, Tag c) => Proxy c -> Proxy q -> Proxy (Body (Body c q) q) -> Int
    inner _ _ = tag

data Evidence b p where
  Have :: (HasBody b p, Tag b) => Evidence b p
  None :: Evidence b p

fromEvidence :: forall b p. Evidence b p -> Int
fromEvidence Have = use inner + tag (Proxy :: Proxy (Body (Body b p) p))
  where
    inner :: forall c q. (HasBody c q, Tag c) => Proxy c -> Proxy q -> Proxy (Body (Body c q) q) -> Int
    inner _ _ = tag
fromEvidence None = 0

data Choice where
  Local :: HasBody c q => Proxy c -> Proxy q -> Choice
  Plain :: Choice

-- The inner clause's equality ends with it; the outer equality stays available
-- in both clauses, including when the second clause introduces no evidence.
preserve :: forall b p. (HasBody b p, Tag b) => Choice -> Proxy (Body (Body b p) p) -> Int
preserve (Local _ _) x = tag x
preserve Plain x = tag x

main :: IO ()
main = print (outer @Int @Parsed Proxy, fromEvidence (Have :: Evidence Bool Parsed),
              fromEvidence (None :: Evidence Int Parsed),
              preserve @Int @Parsed (Local (Proxy :: Proxy Bool) (Proxy :: Proxy Parsed)) Proxy,
              preserve @Bool @Parsed Plain Proxy)
