{-# LANGUAGE AllowAmbiguousTypes, GADTs, MultiParamTypeClasses, NoPolyKinds,
             ScopedTypeVariables, TypeFamilies, UndecidableSuperClasses #-}
module Main where

data Parsed
data Proxy a = Proxy
type family Body b p where Body b Parsed = b
class (b ~ Body b p) => HasBody b p
class Tag a where tag :: Proxy a -> Int
data Evidence b p where
  Have :: HasBody b p => Evidence b p
  None :: Evidence b p

bad :: forall b p. Tag b => Evidence b p -> Int
bad Have = tag (Proxy :: Proxy (Body (Body b p) p))
-- This clause has no HasBody evidence. The previous clause's equality must not
-- make Tag (Body (Body b p) p) look like the available Tag b dictionary.
bad None = tag (Proxy :: Proxy (Body (Body b p) p))

main :: IO ()
main = print ()
