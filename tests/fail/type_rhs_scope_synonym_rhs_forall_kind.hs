{-# LANGUAGE RankNTypes, PolyKinds #-}
import Data.Proxy (Proxy)

type T = forall (a :: k). Proxy a -> Proxy a

main :: IO ()
main = pure ()
