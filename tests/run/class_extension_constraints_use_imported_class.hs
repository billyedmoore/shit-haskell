{-# LANGUAGE NoMultiParamTypeClasses #-}

import DiagnosticClassOrigin (Two)

f :: Two Int Bool => ()
f = ()

main :: IO ()
main = pure ()
