{-# LANGUAGE NoDataKinds #-}

import Data.Proxy

value :: Proxy ('(Int, Bool))
value = Proxy

main :: IO ()
main = pure ()
