{-# LANGUAGE PolyKinds, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy

bad = Proxy @(Int :: missing)

main :: IO ()
main = pure ()
