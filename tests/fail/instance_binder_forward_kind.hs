{-# LANGUAGE PolyKinds #-}

import Data.Proxy

class C a where
  f :: a -> Bool

instance forall (a :: k) k. C (Proxy a) where
  f _ = True

main :: IO ()
main = pure ()
