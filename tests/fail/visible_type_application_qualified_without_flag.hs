{-# LANGUAGE NoImpredicativeTypes, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy

bad = Proxy @(Eq Int => Int)

main :: IO ()
main = pure ()
