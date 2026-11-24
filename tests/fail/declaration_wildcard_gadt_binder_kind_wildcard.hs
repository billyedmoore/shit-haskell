{-# LANGUAGE GADTs, PolyKinds, PartialTypeSignatures #-}
import Data.Proxy (Proxy)

data T where
  T :: forall (a :: _). Proxy a -> T

main :: IO ()
main = pure ()
