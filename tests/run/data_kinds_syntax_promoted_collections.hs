{-# LANGUAGE DataKinds #-}

import Data.Proxy

value :: Proxy '(12, 'True)
value = Proxy

listed :: Proxy [Int, Bool]
listed = Proxy

main :: IO ()
main = pure ()
