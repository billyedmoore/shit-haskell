{-# LANGUAGE TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy

bad = id @(Int -> missing)

main :: IO ()
main = pure ()
