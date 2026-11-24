{-# LANGUAGE DatatypeContexts, PartialTypeSignatures #-}
import Data.Proxy (Proxy)

data Show _ => T a = T a

main :: IO ()
main = pure ()
