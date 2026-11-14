{-# LANGUAGE AllowAmbiguousTypes, FlexibleInstances, MultiParamTypeClasses,
             ScopedTypeVariables, TypeApplications, TypeFamilies,
             UndecidableSuperClasses #-}
module Main where

data Parsed
data Renamed
data Proxy a = Proxy
type family Body b p where
  Body b Parsed = b
  Body b Renamed = [b]
class (b ~ Body b p) => HasBody b p
instance HasBody b Parsed
class Tag a where tag :: Proxy a -> Int
instance Tag Int where tag _ = 17
instance Tag Bool where tag _ = 29

-- The superclass equality is recursive syntactically, but a given allows each
-- nested application to be replaced by b, from the innermost one outwards.
first :: forall b p. (HasBody b p, Tag b) => Proxy (Body (Body (Body b p) p) p) -> Int
first = tag
second :: forall b p. (HasBody b p, Tag b) => Proxy (Body (Body b p) p) -> Int
second = tag

main :: IO ()
main = print (first @Int @Parsed Proxy, second @Bool @Parsed Proxy)
