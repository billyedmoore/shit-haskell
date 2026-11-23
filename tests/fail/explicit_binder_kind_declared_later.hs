{-# LANGUAGE PolyKinds #-}

import Data.Proxy

f :: forall (a :: k) k. Proxy a -> ()
f _ = ()

main :: IO ()
main = pure ()
