{-# LANGUAGE PolyKinds #-}

import Data.Proxy

class C a where
  f :: a -> Bool

instance forall k (a :: k). C (Proxy a) where
  f _ = True

main :: IO ()
main = print (f (Proxy :: Proxy Int))
