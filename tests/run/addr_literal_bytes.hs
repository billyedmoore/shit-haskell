{-# LANGUAGE MagicHash #-}
-- Addr# literals hold bytes: escapes above 0x7f are one byte each (alex's tables).
import GHC.Exts
data A = A Addr#
t :: A
t = A "\xf8\xff\xff\xff\x72\x00\x00\x00\x01\x80"#
main = do
  let A a = t
  print [I# (ord# (indexCharOffAddr# a i)) | I# i <- [0..9]]
  print (I# (int32ToInt# (indexInt32OffAddr# a 0#)), I# (int16ToInt# (indexInt16OffAddr# a 4#)), I# (int16ToInt# (indexInt16OffAddr# a 4#)))
