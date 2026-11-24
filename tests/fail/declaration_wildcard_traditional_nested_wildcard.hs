{-# LANGUAGE PartialTypeSignatures #-}
import Data.Proxy (Proxy)

data T = T (Proxy _)

main :: IO ()
main = pure ()
