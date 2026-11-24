{-# LANGUAGE PolyKinds #-}

import Data.Proxy

f :: forall k (a :: k). Proxy a -> ()
f _ = ()

main :: IO ()
main = print (f (Proxy :: Proxy Bool))
