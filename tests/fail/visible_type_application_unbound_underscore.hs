{-# LANGUAGE NoNamedWildCards, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy

bad = Proxy @_missing

main :: IO ()
main = pure ()
