{-# LANGUAGE PolyKinds #-}
module BootRenamedBinders where

import Data.Proxy (Proxy)

keep :: forall a b. a -> b -> a

kinded :: Proxy (a :: k) -> Proxy k -> Int
