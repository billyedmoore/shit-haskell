{-# LANGUAGE ExplicitForAll, NoImpredicativeTypes, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy

value = Proxy @(forall . Int)

main :: IO ()
main = print value
