{-# LANGUAGE TypeFamilies, PolyKinds, TypeApplications #-}
import Data.Proxy (Proxy)

type family T a where
  T a = Proxy @_ a

main :: IO ()
main = pure ()
