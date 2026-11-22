{-# LANGUAGE PolyKinds #-}
module BootRenamedBinders where

import Data.Proxy (Proxy)

keep :: forall x y. x -> y -> x
keep x _ = x

kinded :: forall k (a :: k). Proxy a -> Proxy k -> Int
kinded _ _ = 7
