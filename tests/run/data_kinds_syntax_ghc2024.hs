{-# LANGUAGE GHC2024 #-}

import Data.Proxy

value :: Proxy '[12, 13]
value = Proxy

main :: IO ()
main = pure ()
