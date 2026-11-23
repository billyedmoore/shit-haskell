{-# LANGUAGE PolyKinds, TypeApplications #-}
import Data.Proxy (Proxy)

type T = Proxy @_ Bool

main :: IO ()
main = pure ()
