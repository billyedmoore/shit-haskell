{-# LANGUAGE PolyKinds, NamedWildCards, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy

value = Proxy @(Int :: _k)

main :: IO ()
main = print value
