{-# LANGUAGE TypeApplications, ImpredicativeTypes, NamedWildCards, PolyKinds #-}
import Data.Proxy

value = Proxy @(forall (x :: _kind). Proxy x)

main :: IO ()
main = case value of Proxy -> putStrLn "ok"
