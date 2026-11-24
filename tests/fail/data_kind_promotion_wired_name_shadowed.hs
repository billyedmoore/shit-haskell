{-# LANGUAGE NoDataKinds #-}

import Data.Proxy
data D = IntRep
value :: Proxy 'IntRep
value = Proxy

main :: IO ()
main = pure ()
